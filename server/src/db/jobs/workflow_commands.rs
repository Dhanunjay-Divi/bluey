const WORKFLOW_COMMAND_PROTOCOL_VERSION: i64 = 2;
const WORKFLOW_COMMAND_REQUEST_MAX_BYTES: usize = 1024 * 1024;
const WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES: usize = 8 * 1024 * 1024;
const WORKFLOW_COMMAND_LEASE_MIN_MS: i64 = 1_000;
const WORKFLOW_COMMAND_LEASE_MAX_MS: i64 = 15 * 60 * 1_000;
const WORKFLOW_COMMAND_RETRY_MAX_MS: i64 = 5 * 60 * 1_000;
const WORKFLOW_COMMAND_SAFE_INTEGER_MAX: i64 = 9_007_199_254_740_991;

const SQLITE_WORKFLOW_APPLICATION_AUTHORITY_SQL: &str =
    "SELECT application_json, state, job_id FROM jobs_applications
      WHERE account_id = ?1 AND id = ?2";
const POSTGRES_WORKFLOW_APPLICATION_AUTHORITY_SQL: &str =
    "SELECT application_json, state, job_id FROM jobs_applications
      WHERE account_id = $1 AND id = $2 FOR UPDATE";
const SQLITE_OPEN_APPLICATION_INTERVENTIONS_SQL: &str = "SELECT COUNT(*) FROM jobs_interventions
      WHERE account_id = ?1 AND application_id = ?2 AND status = 'open'";
const POSTGRES_OPEN_APPLICATION_INTERVENTIONS_SQL: &str =
    "SELECT COUNT(*)::bigint FROM jobs_interventions
      WHERE account_id = $1 AND application_id = $2 AND status = 'open'";

const WORKFLOW_COMMAND_SELECT: &str =
    "id, account_id, application_id, run_id, workflow_id, intervention_id,
     command_kind, protocol_version, idempotency_key_hmac_sha256, request_id,
     request_hmac_sha256, payload_hmac_sha256, state, command_json, attempt_count,
     fence, lease_owner, lease_token_sha256, lease_expires_at_ms, active_attempt_id,
     first_request_started_at_ms, first_ambiguous_at_ms, next_attempt_at_ms,
     last_outcome_code, temporal_run_id, accepted_at_ms, created_at_ms, updated_at_ms";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobsWorkflowCommandKind {
    Start,
    Resume,
}

impl JobsWorkflowCommandKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Resume => "resume",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "start" => Ok(Self::Start),
            "resume" => Ok(Self::Resume),
            _ => anyhow::bail!("invalid workflow command kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobsWorkflowCommandState {
    Pending,
    Claimed,
    Delivering,
    DeliveryUnknown,
    Accepted,
    IdentityConflict,
    Rejected,
    Cancelled,
}

impl JobsWorkflowCommandState {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "delivering" => Ok(Self::Delivering),
            "delivery_unknown" => Ok(Self::DeliveryUnknown),
            "accepted" => Ok(Self::Accepted),
            "identity_conflict" => Ok(Self::IdentityConflict),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            _ => anyhow::bail!("invalid stored workflow command state"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowStartMaterial {
    pub workflow_input: Value,
    pub browser_session_id: String,
    pub result_request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowResumeMaterial {
    pub workflow_input: Value,
    pub browser_session_id: String,
    pub result_request_id: String,
    pub resolution: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operation", content = "material", rename_all = "snake_case")]
pub enum JobsWorkflowCommandPayload {
    Start(JobsWorkflowStartMaterial),
    Resume(JobsWorkflowResumeMaterial),
}

impl JobsWorkflowCommandPayload {
    fn command_kind(&self) -> JobsWorkflowCommandKind {
        match self {
            Self::Start(_) => JobsWorkflowCommandKind::Start,
            Self::Resume(_) => JobsWorkflowCommandKind::Resume,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobsWorkflowCommandEnvelope {
    pub schema_version: i64,
    pub command_kind: JobsWorkflowCommandKind,
    pub request_id: String,
    pub request_hmac_sha256: String,
    pub payload_hmac_sha256: String,
    pub workflow_id: String,
    pub application_id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intervention_id: Option<String>,
    pub payload: JobsWorkflowCommandPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobsWorkflowCommand {
    pub id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub intervention_id: Option<String>,
    pub command_kind: JobsWorkflowCommandKind,
    pub protocol_version: i64,
    pub idempotency_key_hmac_sha256: String,
    pub request_id: String,
    pub request_hmac_sha256: String,
    pub payload_hmac_sha256: String,
    pub state: JobsWorkflowCommandState,
    pub envelope: JobsWorkflowCommandEnvelope,
    pub attempt_count: i64,
    pub fence: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at_ms: Option<i64>,
    pub active_attempt_id: Option<String>,
    pub first_request_started_at_ms: Option<i64>,
    pub first_ambiguous_at_ms: Option<i64>,
    pub next_attempt_at_ms: Option<i64>,
    pub last_outcome_code: Option<String>,
    pub temporal_run_id: Option<String>,
    pub accepted_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct NewJobsWorkflowCommand {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub intervention_id: Option<String>,
    pub command_kind: JobsWorkflowCommandKind,
    /// Public idempotency material is HMACed and is never persisted or returned.
    pub idempotency_key: String,
    /// Canonical request semantics used only for exact-replay authentication.
    pub request: Value,
    /// Frozen gateway command body, encrypted before it is persisted.
    pub payload: JobsWorkflowCommandPayload,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StageCloudWorkflowStart {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub idempotency_key: String,
    pub workflow_input: Value,
    pub browser_session: BrowserSession,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct StageCloudWorkflowResume {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub workflow_id: String,
    pub intervention_id: String,
    pub idempotency_key: String,
    pub workflow_input: Value,
    pub browser_session_id: String,
    pub resolution: Value,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobsWorkflowCommandAdmission {
    pub command: JobsWorkflowCommand,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct PrepareJobsWorkflowIntervention {
    pub request_id: String,
    pub payload_hmac_sha256: String,
    pub workflow_id: String,
    pub command_kind: JobsWorkflowCommandKind,
    pub intervention_id: Option<String>,
    pub receipt: Value,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedJobsWorkflowIntervention {
    pub request_id: String,
    pub payload_hmac_sha256: String,
    pub intervention_id: String,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct PublishJobsWorkflowIntervention {
    pub request_id: String,
    pub payload_hmac_sha256: String,
    pub workflow_id: String,
    pub command_kind: JobsWorkflowCommandKind,
    pub command_intervention_id: Option<String>,
    pub intervention_id: String,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct PublishedJobsWorkflowIntervention {
    pub intervention: Intervention,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsWorkflowTerminalReason {
    RunnerFailed,
    RunnerAmbiguous,
    InterventionTimeout,
    InterventionLimit,
}

impl JobsWorkflowTerminalReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::RunnerFailed => "runner_failed",
            Self::RunnerAmbiguous => "runner_ambiguous",
            Self::InterventionTimeout => "intervention_timeout",
            Self::InterventionLimit => "intervention_limit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsWorkflowTerminalOutcome {
    Failed(JobsWorkflowTerminalReason),
    SideEffectUnknown(JobsWorkflowTerminalReason),
}

impl JobsWorkflowTerminalOutcome {
    fn state(self) -> &'static str {
        match self {
            Self::Failed(_) => "failed",
            Self::SideEffectUnknown(_) => "side_effect_unknown",
        }
    }

    fn reason(self) -> JobsWorkflowTerminalReason {
        match self {
            Self::Failed(reason) | Self::SideEffectUnknown(reason) => reason,
        }
    }

    fn valid(self) -> bool {
        matches!(
            self,
            Self::Failed(
                JobsWorkflowTerminalReason::RunnerFailed
                    | JobsWorkflowTerminalReason::InterventionTimeout
                    | JobsWorkflowTerminalReason::InterventionLimit
            ) | Self::SideEffectUnknown(JobsWorkflowTerminalReason::RunnerAmbiguous)
        )
    }
}

#[derive(Debug, Clone)]
pub struct FinalizeJobsWorkflowExecution {
    pub request_id: String,
    pub payload_hmac_sha256: String,
    pub workflow_id: String,
    pub command_kind: JobsWorkflowCommandKind,
    pub intervention_id: Option<String>,
    pub outcome: JobsWorkflowTerminalOutcome,
    pub open_intervention_id: Option<String>,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsWorkflowExecutionFinalization {
    pub request_id: String,
    pub outcome: JobsWorkflowTerminalOutcome,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct PrepareJobsWorkflowCleanup {
    pub account_id: String,
    pub generation: i64,
    pub legacy_reconciled: bool,
    pub legacy_unresolved_count: i64,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsWorkflowCleanupStatus {
    pub account_id: String,
    pub generation: i64,
    pub target_set_hmac_sha256: String,
    pub target_count: i64,
    pub never_delivered_cancelled: i64,
    pub cleanup_complete: i64,
    pub cleanup_pending: i64,
    pub legacy_reconciled: bool,
    pub legacy_unresolved_count: i64,
    pub complete: bool,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsWorkflowCleanupObservationKind {
    IdentityConfirmed,
    IdentityConflict,
    TerminationRequested,
    TerminationConfirmed,
    HistoryDeleteRequested,
    HistoryDeleteConfirmed,
    AbsenceProved,
}

impl JobsWorkflowCleanupObservationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::IdentityConfirmed => "identity_confirmed",
            Self::IdentityConflict => "identity_conflict",
            Self::TerminationRequested => "termination_requested",
            Self::TerminationConfirmed => "termination_confirmed",
            Self::HistoryDeleteRequested => "history_delete_requested",
            Self::HistoryDeleteConfirmed => "history_delete_confirmed",
            Self::AbsenceProved => "absence_proved",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompleteJobsWorkflowCleanupTarget {
    pub observation_kind: JobsWorkflowCleanupObservationKind,
    /// Required for positive identity and all run-scoped cleanup evidence;
    /// absent only while proving that the opaque workflow identity does not exist.
    pub first_execution_run_id: Option<String>,
    /// Closed, provider-produced proof metadata; HMACed and never stored raw.
    pub evidence: Value,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsWorkflowCleanupObservationReceipt {
    pub account_id: String,
    pub workflow_id: String,
    pub generation: i64,
    pub observation_kind: JobsWorkflowCleanupObservationKind,
    pub replayed: bool,
}

#[derive(Clone)]
pub struct JobsWorkflowCleanupLease {
    pub account_id: String,
    pub workflow_id: String,
    pub start_command_id: String,
    pub start_request_id: String,
    pub start_payload_hmac_sha256: String,
    pub first_execution_run_id: Option<String>,
    pub generation: i64,
    pub target_set_hmac_sha256: String,
    pub cleanup_state: String,
    pub fence: i64,
    pub cleanup_request_id: String,
    pub lease_owner: String,
    pub lease_token: String,
    pub lease_expires_at_ms: i64,
}

impl std::fmt::Debug for JobsWorkflowCleanupLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobsWorkflowCleanupLease")
            .field("account_id", &self.account_id)
            .field("workflow_id", &self.workflow_id)
            .field("start_command_id", &self.start_command_id)
            .field("start_request_id", &self.start_request_id)
            .field("generation", &self.generation)
            .field("fence", &self.fence)
            .field("cleanup_request_id", &self.cleanup_request_id)
            .field("lease_owner", &self.lease_owner)
            .field("lease_token", &"[REDACTED]")
            .field("lease_expires_at_ms", &self.lease_expires_at_ms)
            .finish()
    }
}

pub struct JobsWorkflowCommandLease {
    pub command: JobsWorkflowCommand,
    pub attempt_id: String,
    pub lease_owner: String,
    pub lease_token: String,
    pub fence: i64,
    pub lease_expires_at_ms: i64,
}

impl std::fmt::Debug for JobsWorkflowCommandLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("JobsWorkflowCommandLease")
            .field("command_id", &self.command.id)
            .field("attempt_id", &self.attempt_id)
            .field("lease_owner", &self.lease_owner)
            .field("lease_token", &"[REDACTED]")
            .field("fence", &self.fence)
            .field("lease_expires_at_ms", &self.lease_expires_at_ms)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsWorkflowAcceptedOutcome {
    Accepted,
    AlreadyAccepted,
}

impl JobsWorkflowAcceptedOutcome {
    fn event_kind(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AlreadyAccepted => "already_accepted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsWorkflowUnknownReason {
    TransportTimeout,
    ConnectionLost,
    GatewayUnavailable,
    GatewayServerError,
    MalformedResponse,
}

impl JobsWorkflowUnknownReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::TransportTimeout => "transport_timeout",
            Self::ConnectionLost => "connection_lost",
            Self::GatewayUnavailable => "gateway_unavailable",
            Self::GatewayServerError => "gateway_5xx",
            Self::MalformedResponse => "malformed_response",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsWorkflowRejectionReason {
    InvalidRequest,
    Unauthorized,
    WorkflowNotFound,
    UnsupportedProtocol,
    GatewayRejected,
}

impl JobsWorkflowRejectionReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::WorkflowNotFound => "workflow_not_found",
            Self::UnsupportedProtocol => "unsupported_protocol",
            Self::GatewayRejected => "gateway_rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsWorkflowAcceptanceReceipt {
    pub outcome: JobsWorkflowAcceptedOutcome,
    pub request_id: String,
    pub payload_hmac_sha256: String,
    pub workflow_id: String,
    pub intervention_id: Option<String>,
    pub temporal_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobsWorkflowCommandCompletion {
    Accepted(JobsWorkflowAcceptanceReceipt),
    DeliveryUnknown(JobsWorkflowUnknownReason),
    IdentityConflict,
    Rejected(JobsWorkflowRejectionReason),
}

#[derive(Debug, Error)]
pub enum JobsWorkflowCommandError {
    #[error("invalid workflow command request")]
    InvalidRequest,
    #[error("workflow command identity conflict")]
    IdentityConflict,
    #[error("workflow command not found")]
    NotFound,
    #[error("workflow command is not in the required state")]
    InvalidState,
    #[error("workflow command lease is stale")]
    StaleLease,
    #[error("workflow command lease expired")]
    LeaseExpired,
    #[error("workflow command request-start evidence is missing")]
    RequestNotStarted,
    #[error("workflow cleanup has fenced command materialization")]
    CleanupFenced,
}

#[derive(Debug, Clone)]
struct StoredWorkflowCommandRow {
    id: String,
    account_id: String,
    application_id: String,
    run_id: String,
    workflow_id: String,
    intervention_id: Option<String>,
    command_kind: String,
    protocol_version: i64,
    idempotency_key_hmac_sha256: String,
    request_id: String,
    request_hmac_sha256: String,
    payload_hmac_sha256: String,
    state: String,
    command_json: String,
    attempt_count: i64,
    fence: i64,
    lease_owner: Option<String>,
    lease_token_sha256: Option<String>,
    lease_expires_at_ms: Option<i64>,
    active_attempt_id: Option<String>,
    first_request_started_at_ms: Option<i64>,
    first_ambiguous_at_ms: Option<i64>,
    next_attempt_at_ms: Option<i64>,
    last_outcome_code: Option<String>,
    temporal_run_id: Option<String>,
    accepted_at_ms: Option<i64>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

fn sqlite_workflow_command_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredWorkflowCommandRow> {
    Ok(StoredWorkflowCommandRow {
        id: row.get(0)?,
        account_id: row.get(1)?,
        application_id: row.get(2)?,
        run_id: row.get(3)?,
        workflow_id: row.get(4)?,
        intervention_id: row.get(5)?,
        command_kind: row.get(6)?,
        protocol_version: row.get(7)?,
        idempotency_key_hmac_sha256: row.get(8)?,
        request_id: row.get(9)?,
        request_hmac_sha256: row.get(10)?,
        payload_hmac_sha256: row.get(11)?,
        state: row.get(12)?,
        command_json: row.get(13)?,
        attempt_count: row.get(14)?,
        fence: row.get(15)?,
        lease_owner: row.get(16)?,
        lease_token_sha256: row.get(17)?,
        lease_expires_at_ms: row.get(18)?,
        active_attempt_id: row.get(19)?,
        first_request_started_at_ms: row.get(20)?,
        first_ambiguous_at_ms: row.get(21)?,
        next_attempt_at_ms: row.get(22)?,
        last_outcome_code: row.get(23)?,
        temporal_run_id: row.get(24)?,
        accepted_at_ms: row.get(25)?,
        created_at_ms: row.get(26)?,
        updated_at_ms: row.get(27)?,
    })
}

fn postgres_workflow_command_row(row: &postgres::Row) -> StoredWorkflowCommandRow {
    StoredWorkflowCommandRow {
        id: row.get(0),
        account_id: row.get(1),
        application_id: row.get(2),
        run_id: row.get(3),
        workflow_id: row.get(4),
        intervention_id: row.get(5),
        command_kind: row.get(6),
        protocol_version: row.get(7),
        idempotency_key_hmac_sha256: row.get(8),
        request_id: row.get(9),
        request_hmac_sha256: row.get(10),
        payload_hmac_sha256: row.get(11),
        state: row.get(12),
        command_json: row.get(13),
        attempt_count: row.get(14),
        fence: row.get(15),
        lease_owner: row.get(16),
        lease_token_sha256: row.get(17),
        lease_expires_at_ms: row.get(18),
        active_attempt_id: row.get(19),
        first_request_started_at_ms: row.get(20),
        first_ambiguous_at_ms: row.get(21),
        next_attempt_at_ms: row.get(22),
        last_outcome_code: row.get(23),
        temporal_run_id: row.get(24),
        accepted_at_ms: row.get(25),
        created_at_ms: row.get(26),
        updated_at_ms: row.get(27),
    }
}

fn workflow_command_identifier(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn workflow_command_opaque_identifier(value: &str, max_bytes: usize) -> bool {
    (20..=max_bytes).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn workflow_command_hmac_matches(left: &str, right: &str) -> bool {
    left.len() == right.len() && left.as_bytes().ct_eq(right.as_bytes()).unwrap_u8() == 1
}

fn canonical_workflow_command_value(value: &Value) -> Result<Value> {
    match value {
        Value::Array(values) => Ok(Value::Array(
            values
                .iter()
                .map(canonical_workflow_command_value)
                .collect::<Result<Vec<_>>>()?,
        )),
        Value::Object(values) => {
            let sorted = values
                .iter()
                .map(|(key, value)| Ok((key.clone(), canonical_workflow_command_value(value)?)))
                .collect::<Result<BTreeMap<_, _>>>()?;
            Ok(Value::Object(sorted.into_iter().collect()))
        }
        Value::Number(number) => {
            let valid_signed = number.as_i64().is_some_and(|value| {
                (-WORKFLOW_COMMAND_SAFE_INTEGER_MAX..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX)
                    .contains(&value)
            });
            let valid_unsigned = number
                .as_u64()
                .is_some_and(|value| value <= WORKFLOW_COMMAND_SAFE_INTEGER_MAX as u64);
            if !valid_signed && !valid_unsigned {
                anyhow::bail!("workflow command numbers must be safe integers")
            }
            Ok(value.clone())
        }
        _ => Ok(value.clone()),
    }
}

fn canonical_workflow_command_bytes(
    value: &Value,
    max_bytes: usize,
    label: &str,
) -> Result<Vec<u8>> {
    let canonical = canonical_workflow_command_value(value)?;
    let encoded = serde_json::to_vec(&canonical).with_context(|| format!("serialize {label}"))?;
    if encoded.len() > max_bytes {
        anyhow::bail!("{label} is too large")
    }
    Ok(encoded)
}

fn workflow_command_hmac(scope: &str, value: &Value, max_bytes: usize) -> Result<String> {
    let encoded = canonical_workflow_command_bytes(value, max_bytes, scope)?;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(&jobs_data_key()?)
        .map_err(|_| anyhow::anyhow!("invalid Bluey Jobs data key"))?;
    mac.update(b"bluey-jobs-workflow-command-v2");
    mac.update(&[0]);
    mac.update(scope.as_bytes());
    mac.update(&[0]);
    mac.update(&encoded);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn workflow_command_idempotency_hmac(input: &NewJobsWorkflowCommand) -> Result<String> {
    workflow_command_hmac(
        "idempotency-key",
        &json!({
            "accountId": input.account_id,
            "commandKind": input.command_kind,
            "idempotencyKey": input.idempotency_key,
        }),
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
    )
}

fn workflow_command_request_hmac(input: &NewJobsWorkflowCommand) -> Result<String> {
    workflow_command_hmac(
        "admission-request",
        &json!({
            "accountId": input.account_id,
            "applicationId": input.application_id,
            "runId": input.run_id,
            "workflowId": input.workflow_id,
            "interventionId": input.intervention_id,
            "commandKind": input.command_kind,
            "request": input.request,
        }),
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
    )
}

fn workflow_command_payload_hmac(
    input: &NewJobsWorkflowCommand,
    request_id: &str,
    request_hmac_sha256: &str,
) -> Result<String> {
    let payload =
        serde_json::to_value(&input.payload).context("serialize workflow command material")?;
    workflow_command_hmac(
        "frozen-payload",
        &json!({
            "schemaVersion": WORKFLOW_COMMAND_PROTOCOL_VERSION,
            "accountId": input.account_id,
            "applicationId": input.application_id,
            "runId": input.run_id,
            "workflowId": input.workflow_id,
            "interventionId": input.intervention_id,
            "commandKind": input.command_kind,
            "requestId": request_id,
            "requestHmacSha256": request_hmac_sha256,
            "payload": payload,
        }),
        WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
    )
}

fn validate_new_workflow_command(input: &NewJobsWorkflowCommand) -> Result<()> {
    let intervention_valid = match input.command_kind {
        JobsWorkflowCommandKind::Start => input.intervention_id.is_none(),
        JobsWorkflowCommandKind::Resume => input
            .intervention_id
            .as_deref()
            .is_some_and(|value| workflow_command_opaque_identifier(value, 128)),
    };
    if !workflow_command_identifier(&input.account_id, 128)
        || !workflow_command_identifier(&input.application_id, 128)
        || !workflow_command_opaque_identifier(&input.run_id, 128)
        || !workflow_command_opaque_identifier(&input.workflow_id, 192)
        || input.idempotency_key.trim().is_empty()
        || input.idempotency_key.len() > 256
        || input.payload.command_kind() != input.command_kind
        || !intervention_valid
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    canonical_workflow_command_bytes(
        &input.request,
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
        "workflow command request",
    )?;
    canonical_workflow_command_bytes(
        &serde_json::to_value(&input.payload).context("serialize workflow command material")?,
        WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
        "workflow command payload",
    )?;
    Ok(())
}

pub fn new_jobs_workflow_id() -> String {
    format!("bluey-jobs-v2-{}", uuid::Uuid::new_v4())
}

fn new_jobs_workflow_command_id() -> String {
    format!("wfcmd-v2-{}", uuid::Uuid::new_v4())
}

fn new_jobs_workflow_attempt_id() -> String {
    format!("wfattempt-v2-{}", uuid::Uuid::new_v4())
}

fn new_jobs_workflow_attempt_event_id() -> String {
    format!("wfevent-v2-{}", uuid::Uuid::new_v4())
}

fn new_jobs_workflow_cleanup_observation_id() -> String {
    format!("wfcleanupobs-v2-{}", uuid::Uuid::new_v4())
}

fn new_jobs_workflow_cleanup_request_id() -> String {
    format!("wfcleanupreq-v2-{}", uuid::Uuid::new_v4())
}

fn workflow_command_from_stored(row: StoredWorkflowCommandRow) -> Result<JobsWorkflowCommand> {
    let envelope: JobsWorkflowCommandEnvelope =
        parse_json(row.command_json, "Jobs workflow command")?;
    let command_kind = JobsWorkflowCommandKind::parse(&row.command_kind)?;
    let state = JobsWorkflowCommandState::parse(&row.state)?;
    if row.protocol_version != WORKFLOW_COMMAND_PROTOCOL_VERSION
        || envelope.schema_version != WORKFLOW_COMMAND_PROTOCOL_VERSION
        || envelope.command_kind != command_kind
        || envelope.request_id != row.request_id
        || !workflow_command_hmac_matches(&envelope.request_hmac_sha256, &row.request_hmac_sha256)
        || !workflow_command_hmac_matches(&envelope.payload_hmac_sha256, &row.payload_hmac_sha256)
        || envelope.workflow_id != row.workflow_id
        || envelope.application_id != row.application_id
        || envelope.run_id != row.run_id
        || envelope.intervention_id != row.intervention_id
    {
        anyhow::bail!("stored Jobs workflow command authority changed")
    }
    Ok(JobsWorkflowCommand {
        id: row.id,
        account_id: row.account_id,
        application_id: row.application_id,
        run_id: row.run_id,
        workflow_id: row.workflow_id,
        intervention_id: row.intervention_id,
        command_kind,
        protocol_version: row.protocol_version,
        idempotency_key_hmac_sha256: row.idempotency_key_hmac_sha256,
        request_id: row.request_id,
        request_hmac_sha256: row.request_hmac_sha256,
        payload_hmac_sha256: row.payload_hmac_sha256,
        state,
        envelope,
        attempt_count: row.attempt_count,
        fence: row.fence,
        lease_owner: row.lease_owner,
        lease_expires_at_ms: row.lease_expires_at_ms,
        active_attempt_id: row.active_attempt_id,
        first_request_started_at_ms: row.first_request_started_at_ms,
        first_ambiguous_at_ms: row.first_ambiguous_at_ms,
        next_attempt_at_ms: row.next_attempt_at_ms,
        last_outcome_code: row.last_outcome_code,
        temporal_run_id: row.temporal_run_id,
        accepted_at_ms: row.accepted_at_ms,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}

fn workflow_command_replay(
    stored: StoredWorkflowCommandRow,
    input: &NewJobsWorkflowCommand,
    request_hmac_sha256: &str,
) -> Result<JobsWorkflowCommandAdmission> {
    let payload_hmac_sha256 =
        workflow_command_payload_hmac(input, &stored.request_id, request_hmac_sha256)?;
    if stored.account_id != input.account_id
        || stored.application_id != input.application_id
        || stored.run_id != input.run_id
        || stored.workflow_id != input.workflow_id
        || stored.intervention_id != input.intervention_id
        || stored.command_kind != input.command_kind.as_str()
        || !workflow_command_hmac_matches(&stored.request_hmac_sha256, request_hmac_sha256)
        || !workflow_command_hmac_matches(&stored.payload_hmac_sha256, &payload_hmac_sha256)
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let command = workflow_command_from_stored(stored)?;
    if canonical_workflow_command_value(&serde_json::to_value(&command.envelope.payload)?)?
        != canonical_workflow_command_value(&serde_json::to_value(&input.payload)?)?
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    Ok(JobsWorkflowCommandAdmission {
        command,
        replayed: true,
    })
}

fn workflow_command_envelope(
    input: &NewJobsWorkflowCommand,
    request_id: String,
    request_hmac_sha256: String,
    payload_hmac_sha256: String,
) -> JobsWorkflowCommandEnvelope {
    JobsWorkflowCommandEnvelope {
        schema_version: WORKFLOW_COMMAND_PROTOCOL_VERSION,
        command_kind: input.command_kind,
        request_id,
        request_hmac_sha256,
        payload_hmac_sha256,
        workflow_id: input.workflow_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        intervention_id: input.intervention_id.clone(),
        payload: input.payload.clone(),
    }
}

fn stage_workflow_command_request(input: &NewJobsWorkflowCommand) -> Value {
    let (workflow_input, browser_session_id, resolution) = match &input.payload {
        JobsWorkflowCommandPayload::Start(material) => (
            &material.workflow_input,
            material.browser_session_id.as_str(),
            None,
        ),
        JobsWorkflowCommandPayload::Resume(material) => (
            &material.workflow_input,
            material.browser_session_id.as_str(),
            Some(&material.resolution),
        ),
    };
    json!({
        "schemaVersion": WORKFLOW_COMMAND_PROTOCOL_VERSION,
        "commandKind": input.command_kind,
        "applicationId": input.application_id,
        "runId": input.run_id,
        "workflowId": input.workflow_id,
        "interventionId": input.intervention_id,
        "workflowInput": workflow_input,
        "browserSessionId": browser_session_id,
        "resolution": resolution,
    })
}

fn stage_workflow_command_payload(
    command_kind: JobsWorkflowCommandKind,
    workflow_input: Value,
    browser_session_id: String,
    result_request_id: String,
    resolution: Option<Value>,
) -> Result<JobsWorkflowCommandPayload> {
    match command_kind {
        JobsWorkflowCommandKind::Start if resolution.is_none() => Ok(
            JobsWorkflowCommandPayload::Start(JobsWorkflowStartMaterial {
                workflow_input,
                browser_session_id,
                result_request_id,
            }),
        ),
        JobsWorkflowCommandKind::Resume => Ok(JobsWorkflowCommandPayload::Resume(
            JobsWorkflowResumeMaterial {
                workflow_input,
                browser_session_id,
                result_request_id,
                resolution: resolution.ok_or(JobsWorkflowCommandError::InvalidRequest)?,
            },
        )),
        _ => Err(JobsWorkflowCommandError::InvalidRequest.into()),
    }
}

fn workflow_command_request_id_for_idempotency(input: &NewJobsWorkflowCommand) -> Result<String> {
    let idempotency_hmac = workflow_command_idempotency_hmac(input)?;
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hex::decode(idempotency_hmac)?[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!("wfreq-v2-{}", uuid::Uuid::from_bytes(bytes)))
}

fn workflow_cleanup_frozen_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM jobs_workflow_cleanup_generations WHERE account_id = ?1
         )",
        params![account_id],
        |row| row.get(0),
    )?)
}

fn workflow_cleanup_frozen_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_workflow_cleanup_generations WHERE account_id = $1
             )",
            &[&account_id],
        )?
        .get(0))
}

fn require_no_workflow_cleanup_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    if workflow_cleanup_frozen_sqlite_tx(tx, account_id)? {
        return Err(JobsWorkflowCommandError::CleanupFenced.into());
    }
    Ok(())
}

fn require_no_workflow_cleanup_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    if workflow_cleanup_frozen_postgres_tx(tx, account_id)? {
        return Err(JobsWorkflowCommandError::CleanupFenced.into());
    }
    Ok(())
}

fn workflow_distribution_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn cloud_distribution_ready_sqlite_tx(tx: &rusqlite::Transaction<'_>) -> Result<bool> {
    if crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission().is_none() {
        return Ok(false);
    }
    Ok(
        workflow_distribution_flag_enabled("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED")
            && crate::jobs_workflow_dispatch::workflow_command_dispatch_configured_for_admission()
            && sqlite_runner_volume_fleet_distribution_ready(tx)?,
    )
}

fn cloud_distribution_ready_postgres_tx(tx: &mut postgres::Transaction<'_>) -> Result<bool> {
    if crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission().is_none() {
        return Ok(false);
    }
    Ok(
        workflow_distribution_flag_enabled("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED")
            && crate::jobs_workflow_dispatch::workflow_command_dispatch_configured_for_admission()
            && postgres_runner_volume_fleet_distribution_ready(tx)?,
    )
}

fn load_stage_application_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<(JobApplication, JobPosting)> {
    let row = tx
        .query_row(
            "SELECT application.application_json, application.job_id,
                    posting.posting_json
               FROM jobs_applications application
               JOIN jobs_postings posting
                 ON posting.id = application.job_id
                AND posting.account_id = application.account_id
              WHERE application.account_id = ?1 AND application.id = ?2",
            params![account_id, application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or(JobsWorkflowCommandError::NotFound)?;
    Ok((
        parse_application_json(row.0, application_id, &row.1, "Jobs staged application")?,
        parse_json(row.2, "Jobs staged posting")?,
    ))
}

fn load_stage_application_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<(JobApplication, JobPosting)> {
    let row = tx
        .query_opt(
            "SELECT application.application_json, application.job_id,
                    posting.posting_json
               FROM jobs_applications application
               JOIN jobs_postings posting
                 ON posting.id = application.job_id
                AND posting.account_id = application.account_id
              WHERE application.account_id = $1 AND application.id = $2
              FOR UPDATE OF application",
            &[&account_id, &application_id],
        )?
        .ok_or(JobsWorkflowCommandError::NotFound)?;
    let job_id = row.get::<_, String>(1);
    Ok((
        parse_application_json(
            row.get(0),
            application_id,
            &job_id,
            "Jobs staged application",
        )?,
        parse_json(row.get(2), "Jobs staged posting")?,
    ))
}

fn stage_attempt_reservation_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &StageCloudWorkflowStart,
    application: &JobApplication,
    posting: &JobPosting,
) -> Result<()> {
    if let Some((runner, status)) = tx
        .query_row(
            "SELECT runner, status FROM jobs_attempt_reservations
              WHERE account_id = ?1 AND application_id = ?2",
            params![input.account_id, input.application_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if runner == "unassigned" && status == "reserved" {
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET runner = 'cloud', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2
                    AND runner = 'unassigned' AND status = 'reserved'",
                params![input.account_id, input.application_id, input.now_ms],
            )?;
            if changed != 1 {
                anyhow::bail!("application attempt changed before cloud binding")
            }
            return Ok(());
        }
        if active_attempt_status(&status) {
            anyhow::bail!("application attempt is already active in another runner")
        }
    }
    let preferences_json: String = tx.query_row(
        "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
        params![input.account_id],
        |row| row.get(0),
    )?;
    let preferences: JobPreferences = parse_json(preferences_json, "Jobs preferences")?;
    let period_key = attempt_period_key(input.now_ms, preferences.time_zone_offset_minutes);
    let daily_limit = preferences.daily_limit.clamp(1, 50);
    let used: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_attempt_reservations
          WHERE account_id = ?1 AND period_key = ?2
            AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
        params![input.account_id, period_key],
        |row| row.get(0),
    )?;
    if used >= daily_limit {
        anyhow::bail!("today's application attempt limit has been reached")
    }
    let company_key = normalize_company_key(&posting.company);
    let company_in_use: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_attempt_reservations
          WHERE account_id = ?1 AND company_key = ?2 AND application_id <> ?3
            AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
        params![input.account_id, company_key, input.application_id],
        |row| row.get(0),
    )?;
    if company_in_use > 0 {
        anyhow::bail!("an in-progress or submitted application already exists for this company")
    }
    tx.execute(
        "INSERT INTO jobs_attempt_reservations (
            id, account_id, application_id, company_key, period_key, runner, status,
            reserved_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'cloud', 'reserved', ?6, ?6)
         ON CONFLICT(account_id, application_id) DO UPDATE SET
            company_key = excluded.company_key, period_key = excluded.period_key,
            runner = 'cloud', status = 'reserved',
            reserved_at_ms = excluded.reserved_at_ms, updated_at_ms = excluded.updated_at_ms",
        params![
            format!("attempt-{}", application.id),
            input.account_id,
            input.application_id,
            company_key,
            period_key,
            input.now_ms,
        ],
    )?;
    Ok(())
}

fn stage_attempt_reservation_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &StageCloudWorkflowStart,
    application: &JobApplication,
    posting: &JobPosting,
) -> Result<()> {
    if let Some(row) = tx.query_opt(
        "SELECT runner, status FROM jobs_attempt_reservations
          WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
        &[&input.account_id, &input.application_id],
    )? {
        let runner: String = row.get(0);
        let status: String = row.get(1);
        if runner == "unassigned" && status == "reserved" {
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET runner = 'cloud', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2
                    AND runner = 'unassigned' AND status = 'reserved'",
                &[&input.account_id, &input.application_id, &input.now_ms],
            )?;
            if changed != 1 {
                anyhow::bail!("application attempt changed before cloud binding")
            }
            return Ok(());
        }
        if active_attempt_status(&status) {
            anyhow::bail!("application attempt is already active in another runner")
        }
    }
    let preferences_json: String = tx
        .query_one(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
            &[&input.account_id],
        )?
        .get(0);
    let preferences: JobPreferences = parse_json(preferences_json, "Jobs preferences")?;
    let period_key = attempt_period_key(input.now_ms, preferences.time_zone_offset_minutes);
    let daily_limit = preferences.daily_limit.clamp(1, 50);
    let used: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_attempt_reservations
              WHERE account_id = $1 AND period_key = $2
                AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
            &[&input.account_id, &period_key],
        )?
        .get(0);
    if used >= daily_limit {
        anyhow::bail!("today's application attempt limit has been reached")
    }
    let company_key = normalize_company_key(&posting.company);
    let company_in_use: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_attempt_reservations
              WHERE account_id = $1 AND company_key = $2 AND application_id <> $3
                AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
            &[&input.account_id, &company_key, &input.application_id],
        )?
        .get(0);
    if company_in_use > 0 {
        anyhow::bail!("an in-progress or submitted application already exists for this company")
    }
    tx.execute(
        "INSERT INTO jobs_attempt_reservations (
            id, account_id, application_id, company_key, period_key, runner, status,
            reserved_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, $4, $5, 'cloud', 'reserved', $6, $6)
         ON CONFLICT(account_id, application_id) DO UPDATE SET
            company_key = EXCLUDED.company_key, period_key = EXCLUDED.period_key,
            runner = 'cloud', status = 'reserved',
            reserved_at_ms = EXCLUDED.reserved_at_ms,
            updated_at_ms = EXCLUDED.updated_at_ms",
        &[
            &format!("attempt-{}", application.id),
            &input.account_id,
            &input.application_id,
            &company_key,
            &period_key,
            &input.now_ms,
        ],
    )?;
    Ok(())
}

fn stage_packet_metering_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &StageCloudWorkflowStart,
    application: &JobApplication,
) -> Result<()> {
    if application.resume_version_id.is_none() {
        anyhow::bail!("application packet has no job-specific resume")
    }
    if tx
        .query_row(
            "SELECT 1 FROM jobs_packet_metering WHERE account_id = ?1 AND job_id = ?2",
            params![input.account_id, application.job_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(());
    }
    let (used, limit, period_start): (i64, i64, i64) = tx.query_row(
        "SELECT used_packets, monthly_packet_limit, period_start_ms
           FROM jobs_entitlements WHERE account_id = ?1",
        params![input.account_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let allowance: Option<(String, i64)> = tx
        .query_row(
            "SELECT status, period_start_ms
               FROM jobs_generation_allowance_reservations
              WHERE account_id = ?1 AND job_id = ?2",
            params![input.account_id, application.job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if allowance
        .as_ref()
        .is_some_and(|(status, _)| status == "committed")
    {
        anyhow::bail!("committed Jobs allowance has no packet metering row")
    }
    let pre_reserved = allowance
        .as_ref()
        .is_some_and(|(status, held_period)| status == "reserved" && *held_period == period_start);
    let included = pre_reserved || used < limit;
    let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
    let metering_key = format!("jobs-packet:{}", application.job_id);
    if amount_cents > 0 {
        let balance_before: i64 = tx.query_row(
            "SELECT balance_cents FROM accounts WHERE id = ?1",
            params![input.account_id],
            |row| row.get(0),
        )?;
        if tx.execute(
            "UPDATE accounts SET balance_cents = balance_cents - ?1
              WHERE id = ?2 AND balance_cents >= ?1",
            params![amount_cents, input.account_id],
        )? == 0
        {
            anyhow::bail!("insufficient Bluey balance for Jobs overage")
        }
        crate::db::balance::consume_credit_batches_tx(tx, &input.account_id, amount_cents)?;
        crate::db::balance::insert_balance_ledger_sqlite_tx(
            tx,
            crate::db::balance::BalanceLedgerEntry {
                account_id: &input.account_id,
                event_type: "jobs_packet_overage",
                amount_cents: -amount_cents,
                balance_cents_before: balance_before,
                balance_cents_after: balance_before - amount_cents,
                reason: Some("jobs_completed_packet"),
                provider: None,
                processor_payment_id: None,
                source_id: Some(&application.job_id),
                idempotency_key: Some(&metering_key),
                request_id: Some(&metering_key),
                metadata_json: Some("{\"product\":\"bluey_jobs\"}"),
            },
        )?;
    }
    tx.execute(
        "INSERT INTO jobs_packet_metering (
            account_id, job_id, application_id, metering_key,
            included, amount_cents, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            input.account_id,
            application.job_id,
            application.id,
            metering_key,
            i64::from(included),
            amount_cents,
            input.now_ms,
        ],
    )?;
    tx.execute(
        "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
            updated_at_ms = ?2 WHERE account_id = ?1 AND ?3 = 0",
        params![input.account_id, input.now_ms, i64::from(pre_reserved)],
    )?;
    if allowance
        .as_ref()
        .is_some_and(|(status, _)| status == "reserved")
    {
        tx.execute(
            "UPDATE jobs_generation_allowance_reservations
                SET status = 'committed', application_id = ?3, updated_at_ms = ?4
              WHERE account_id = ?1 AND job_id = ?2 AND status = 'reserved'",
            params![
                input.account_id,
                application.job_id,
                application.id,
                input.now_ms,
            ],
        )?;
    }
    Ok(())
}

fn stage_packet_metering_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &StageCloudWorkflowStart,
    application: &JobApplication,
) -> Result<()> {
    if application.resume_version_id.is_none() {
        anyhow::bail!("application packet has no job-specific resume")
    }
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&format!(
            "jobs-allowance:{}:{}",
            input.account_id, application.job_id
        )],
    )?;
    let entitlement = tx.query_one(
        "SELECT used_packets, monthly_packet_limit, period_start_ms
           FROM jobs_entitlements WHERE account_id = $1 FOR UPDATE",
        &[&input.account_id],
    )?;
    if tx
        .query_opt(
            "SELECT 1 FROM jobs_packet_metering WHERE account_id = $1 AND job_id = $2",
            &[&input.account_id, &application.job_id],
        )?
        .is_some()
    {
        return Ok(());
    }
    let used: i64 = entitlement.get(0);
    let limit: i64 = entitlement.get(1);
    let period_start: i64 = entitlement.get(2);
    let allowance = tx.query_opt(
        "SELECT status, period_start_ms
           FROM jobs_generation_allowance_reservations
          WHERE account_id = $1 AND job_id = $2 FOR UPDATE",
        &[&input.account_id, &application.job_id],
    )?;
    if allowance
        .as_ref()
        .is_some_and(|row| row.get::<_, String>(0) == "committed")
    {
        anyhow::bail!("committed Jobs allowance has no packet metering row")
    }
    let pre_reserved = allowance.as_ref().is_some_and(|row| {
        row.get::<_, String>(0) == "reserved" && row.get::<_, i64>(1) == period_start
    });
    let included = pre_reserved || used < limit;
    let included_db = i32::from(included);
    let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
    let metering_key = format!("jobs-packet:{}", application.job_id);
    if amount_cents > 0 {
        let balance_before: i64 = tx
            .query_one(
                "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                &[&input.account_id],
            )?
            .get(0);
        if tx.execute(
            "UPDATE accounts SET balance_cents = balance_cents - $1
              WHERE id = $2 AND balance_cents >= $1",
            &[&amount_cents, &input.account_id],
        )? == 0
        {
            anyhow::bail!("insufficient Bluey balance for Jobs overage")
        }
        crate::db::balance::consume_credit_batches_pg_tx(tx, &input.account_id, amount_cents)?;
        crate::db::balance::insert_balance_ledger_pg_tx(
            tx,
            crate::db::balance::BalanceLedgerEntry {
                account_id: &input.account_id,
                event_type: "jobs_packet_overage",
                amount_cents: -amount_cents,
                balance_cents_before: balance_before,
                balance_cents_after: balance_before - amount_cents,
                reason: Some("jobs_completed_packet"),
                provider: None,
                processor_payment_id: None,
                source_id: Some(&application.job_id),
                idempotency_key: Some(&metering_key),
                request_id: Some(&metering_key),
                metadata_json: Some("{\"product\":\"bluey_jobs\"}"),
            },
        )?;
    }
    tx.execute(
        "INSERT INTO jobs_packet_metering (
            account_id, job_id, application_id, metering_key,
            included, amount_cents, created_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &input.account_id,
            &application.job_id,
            &application.id,
            &metering_key,
            &included_db,
            &amount_cents,
            &input.now_ms,
        ],
    )?;
    tx.execute(
        "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
            updated_at_ms = $2 WHERE account_id = $1 AND $3 = 0",
        &[&input.account_id, &input.now_ms, &i32::from(pre_reserved)],
    )?;
    if allowance
        .as_ref()
        .is_some_and(|row| row.get::<_, String>(0) == "reserved")
    {
        tx.execute(
            "UPDATE jobs_generation_allowance_reservations
                SET status = 'committed', application_id = $3, updated_at_ms = $4
              WHERE account_id = $1 AND job_id = $2 AND status = 'reserved'",
            &[
                &input.account_id,
                &application.job_id,
                &application.id,
                &input.now_ms,
            ],
        )?;
    }
    Ok(())
}

fn stage_start_rows_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &StageCloudWorkflowStart,
    application: &mut JobApplication,
) -> Result<()> {
    let mut session = input.browser_session.clone();
    if session.id != format!("cloud-{}", input.application_id)
        || session.runner != "cloud"
        || session.application_id.as_deref() != Some(input.application_id.as_str())
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    if application.state != "queued" {
        anyhow::bail!("application is not ready to queue")
    }
    if application
        .run_id
        .as_deref()
        .is_some_and(|run_id| run_id != input.run_id)
    {
        anyhow::bail!("application is already bound to another browser run")
    }
    if let Some((runner, status, session_json)) = tx
        .query_row(
            "SELECT runner, status, session_json FROM jobs_browser_sessions WHERE id = ?1",
            params![session.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
    {
        let existing: BrowserSession = parse_json(session_json, "staged Jobs browser session")?;
        if runner != "cloud"
            || status != "queued"
            || existing.runner != "cloud"
            || existing.status != "queued"
            || existing.application_id.as_deref() != Some(input.application_id.as_str())
        {
            anyhow::bail!("browser session conflicts with an existing execution")
        }
    }
    application.state = "queued".to_string();
    application.run_id = Some(input.run_id.clone());
    application.updated_at_ms = input
        .now_ms
        .max(application.updated_at_ms.saturating_add(1));
    session.created_at_ms = if session.created_at_ms == 0 {
        input.now_ms
    } else {
        session.created_at_ms
    };
    session.updated_at_ms = input.now_ms;
    let application_json = to_json(application, "staged Jobs application")?;
    let session_json = to_json(&session, "staged Jobs browser session")?;
    let changed = tx.execute(
        "UPDATE jobs_applications
            SET state = 'queued', application_json = ?1, updated_at_ms = ?2
          WHERE account_id = ?3 AND id = ?4 AND state = 'queued'",
        params![
            application_json,
            application.updated_at_ms,
            input.account_id,
            input.application_id,
        ],
    )?;
    if changed != 1 {
        anyhow::bail!("application changed before workflow command admission")
    }
    let session_changed = tx.execute(
        "INSERT INTO jobs_browser_sessions (
            id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, 'cloud', ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET status = excluded.status,
            session_json = excluded.session_json, updated_at_ms = excluded.updated_at_ms
         WHERE jobs_browser_sessions.account_id = excluded.account_id
           AND jobs_browser_sessions.runner = 'cloud'
           AND jobs_browser_sessions.status = 'queued'",
        params![
            session.id,
            input.account_id,
            session.status,
            session_json,
            session.created_at_ms,
            session.updated_at_ms,
        ],
    )?;
    if session_changed != 1 {
        anyhow::bail!("browser session conflicts with an existing execution")
    }
    Ok(())
}

fn stage_start_rows_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &StageCloudWorkflowStart,
    application: &mut JobApplication,
) -> Result<()> {
    let mut session = input.browser_session.clone();
    if session.id != format!("cloud-{}", input.application_id)
        || session.runner != "cloud"
        || session.application_id.as_deref() != Some(input.application_id.as_str())
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    if application.state != "queued" {
        anyhow::bail!("application is not ready to queue")
    }
    if application
        .run_id
        .as_deref()
        .is_some_and(|run_id| run_id != input.run_id)
    {
        anyhow::bail!("application is already bound to another browser run")
    }
    if let Some(row) = tx.query_opt(
        "SELECT runner, status, session_json
           FROM jobs_browser_sessions WHERE id = $1 FOR UPDATE",
        &[&session.id],
    )? {
        let runner: String = row.get(0);
        let status: String = row.get(1);
        let existing: BrowserSession = parse_json(row.get(2), "staged Jobs browser session")?;
        if runner != "cloud"
            || status != "queued"
            || existing.runner != "cloud"
            || existing.status != "queued"
            || existing.application_id.as_deref() != Some(input.application_id.as_str())
        {
            anyhow::bail!("browser session conflicts with an existing execution")
        }
    }
    application.state = "queued".to_string();
    application.run_id = Some(input.run_id.clone());
    application.updated_at_ms = input
        .now_ms
        .max(application.updated_at_ms.saturating_add(1));
    session.created_at_ms = if session.created_at_ms == 0 {
        input.now_ms
    } else {
        session.created_at_ms
    };
    session.updated_at_ms = input.now_ms;
    let application_json = to_json(application, "staged Jobs application")?;
    let session_json = to_json(&session, "staged Jobs browser session")?;
    let changed = tx.execute(
        "UPDATE jobs_applications
            SET state = 'queued', application_json = $1, updated_at_ms = $2
          WHERE account_id = $3 AND id = $4 AND state = 'queued'",
        &[
            &application_json,
            &application.updated_at_ms,
            &input.account_id,
            &input.application_id,
        ],
    )?;
    if changed != 1 {
        anyhow::bail!("application changed before workflow command admission")
    }
    let session_changed = tx.execute(
        "INSERT INTO jobs_browser_sessions (
            id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
         ) VALUES ($1, $2, 'cloud', $3, $4, $5, $6)
         ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
            session_json = EXCLUDED.session_json, updated_at_ms = EXCLUDED.updated_at_ms
         WHERE jobs_browser_sessions.account_id = EXCLUDED.account_id
           AND jobs_browser_sessions.runner = 'cloud'
           AND jobs_browser_sessions.status = 'queued'",
        &[
            &session.id,
            &input.account_id,
            &session.status,
            &session_json,
            &session.created_at_ms,
            &session.updated_at_ms,
        ],
    )?;
    if session_changed != 1 {
        anyhow::bail!("browser session conflicts with an existing execution")
    }
    Ok(())
}

fn validate_stage_workflow_input(
    input: &StageCloudWorkflowStart,
    application: &JobApplication,
    posting: &JobPosting,
) -> Result<()> {
    let workflow = input
        .workflow_input
        .as_object()
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    let expected_keys = [
        "accountId",
        "applicationId",
        "applicationIdentityId",
        "browserProfileId",
        "canonicalJobKey",
        "idempotencyKey",
        "job",
        "jobId",
        "packet",
        "packetId",
        "runner",
        "url",
    ];
    if workflow.len() != expected_keys.len()
        || !expected_keys.iter().all(|key| workflow.contains_key(*key))
        || workflow.get("accountId").and_then(Value::as_str) != Some(input.account_id.as_str())
        || workflow.get("applicationId").and_then(Value::as_str)
            != Some(input.application_id.as_str())
        || workflow.get("jobId").and_then(Value::as_str) != Some(application.job_id.as_str())
        || workflow.get("canonicalJobKey").and_then(Value::as_str)
            != Some(posting.canonical_key.as_str())
        || workflow.get("idempotencyKey").and_then(Value::as_str) != Some(input.run_id.as_str())
        || workflow.get("runner").and_then(Value::as_str) != Some("cloud")
        || workflow.get("url").and_then(Value::as_str) != Some(posting.canonical_url.as_str())
        || workflow.get("packetId").and_then(Value::as_str)
            != application.resume_version_id.as_deref()
    {
        anyhow::bail!("workflow input does not match locked Jobs authority")
    }
    let approved = application
        .receipt
        .get("approved_execution")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("approved execution is missing"))?;
    let approved_packet = approved
        .get("packet")
        .ok_or_else(|| anyhow::anyhow!("approved execution packet is missing"))?;
    let approved_job = approved
        .get("job")
        .ok_or_else(|| anyhow::anyhow!("approved execution job is missing"))?;
    let workflow_packet = workflow
        .get("packet")
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    let workflow_job = workflow
        .get("job")
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if canonical_workflow_command_value(approved_job)?
        != canonical_workflow_command_value(workflow_job)?
        || workflow_packet
            .pointer("/applicationId")
            .and_then(Value::as_str)
            != Some(input.application_id.as_str())
        || workflow_packet.pointer("/jobId").and_then(Value::as_str)
            != Some(application.job_id.as_str())
        || workflow_packet
            .pointer("/resumeVersionId")
            .and_then(Value::as_str)
            != application.resume_version_id.as_deref()
    {
        anyhow::bail!("workflow input changed from the frozen approved execution")
    }
    let approved_checksum = approved
        .get("checksum")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64)
        .ok_or_else(|| anyhow::anyhow!("approved execution checksum is missing"))?;
    let transported_checksum = workflow_packet
        .get("approvedPacketChecksum")
        .and_then(Value::as_str)
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if !workflow_command_hmac_matches(approved_checksum, transported_checksum) {
        anyhow::bail!("workflow input approved execution checksum changed")
    }
    let approved_schema_version = approved
        .get("schema_version")
        .and_then(Value::as_i64)
        .filter(|value| matches!(*value, 1..=3))
        .ok_or_else(|| anyhow::anyhow!("approved execution schema is missing"))?;
    if workflow_packet
        .get("approvedExecutionSchemaVersion")
        .and_then(Value::as_i64)
        != Some(approved_schema_version)
    {
        anyhow::bail!("workflow input approved execution schema changed")
    }
    let transported_admission = workflow_packet.get("approvedExecutionAdmission");
    let frozen_admission = approved.get("admission");
    if matches!(approved_schema_version, 2 | 3) {
        if transported_admission.is_none()
            || canonical_workflow_command_value(
                transported_admission.expect("checked approved execution admission"),
            )? != canonical_workflow_command_value(
                frozen_admission
                    .ok_or_else(|| anyhow::anyhow!("approved execution admission is missing"))?,
            )?
        {
            anyhow::bail!("workflow input approved execution admission changed")
        }
    } else if transported_admission.is_some() {
        anyhow::bail!("legacy workflow input gained an execution admission")
    }
    let mut comparable_packet = workflow_packet.clone();
    if let Some(packet) = comparable_packet.as_object_mut() {
        packet.remove("approvedPacketChecksum");
        packet.remove("approvedExecutionSchemaVersion");
        packet.remove("approvedExecutionAdmission");
        packet.remove("executionAuthority");
    }
    if canonical_workflow_command_value(approved_packet)?
        != canonical_workflow_command_value(&comparable_packet)?
    {
        anyhow::bail!("workflow input packet changed from approved execution")
    }
    Ok(())
}

fn new_stage_cloud_workflow_start_command(
    input: &StageCloudWorkflowStart,
) -> Result<NewJobsWorkflowCommand> {
    let mut seed = NewJobsWorkflowCommand {
        account_id: input.account_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        intervention_id: None,
        command_kind: JobsWorkflowCommandKind::Start,
        idempotency_key: input.idempotency_key.clone(),
        request: Value::Null,
        payload: JobsWorkflowCommandPayload::Start(JobsWorkflowStartMaterial {
            workflow_input: input.workflow_input.clone(),
            browser_session_id: input.browser_session.id.clone(),
            result_request_id: String::new(),
        }),
        now_ms: input.now_ms,
    };
    seed.request = stage_workflow_command_request(&seed);
    let result_request_id = workflow_command_request_id_for_idempotency(&seed)?;
    let command = NewJobsWorkflowCommand {
        payload: stage_workflow_command_payload(
            JobsWorkflowCommandKind::Start,
            input.workflow_input.clone(),
            input.browser_session.id.clone(),
            result_request_id,
            None,
        )?,
        ..seed
    };
    validate_new_workflow_command(&command)?;
    Ok(command)
}

pub fn stage_cloud_workflow_start(
    pool: &DbPool,
    input: &StageCloudWorkflowStart,
) -> Result<JobsWorkflowCommandAdmission> {
    if !workflow_command_identifier(&input.browser_session.id, 128) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let frozen_command = new_stage_cloud_workflow_start_command(input)?;
    let frozen_idempotency_hmac = workflow_command_idempotency_hmac(&frozen_command)?;
    let frozen_request_hmac = workflow_command_request_hmac(&frozen_command)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(existing) = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE account_id = ?1 AND command_kind = 'start'
                            AND idempotency_key_hmac_sha256 = ?2"
                    ),
                    params![input.account_id, frozen_idempotency_hmac],
                    sqlite_workflow_command_row,
                )
                .optional()?
            {
                let replay =
                    workflow_command_replay(existing, &frozen_command, &frozen_request_hmac)?;
                let managed_cloud_scope =
                    crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                        .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
                require_managed_cloud_workflow_binding_replay_sqlite_tx(
                    &transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            let managed_cloud_scope =
                crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                    .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &transaction,
                &input.account_id,
            )?;
            require_no_workflow_cleanup_sqlite_tx(&transaction, &input.account_id)?;
            let cloud_browser: i64 = transaction.query_row(
                "SELECT cloud_browser FROM jobs_entitlements WHERE account_id = ?1",
                params![input.account_id],
                |row| row.get(0),
            )?;
            if cloud_browser == 0 || !cloud_distribution_ready_sqlite_tx(&transaction)? {
                anyhow::bail!("cloud browser distribution is unavailable")
            }
            let (mut application, posting) = load_stage_application_sqlite_tx(
                &transaction,
                &input.account_id,
                &input.application_id,
            )?;
            if !current_execution_authorized_sqlite(
                &transaction,
                &input.account_id,
                &application,
                ExecutionAuthorityRunner::Cloud,
            )? {
                anyhow::bail!("current Jobs execution authority does not permit cloud queueing")
            }
            validate_stage_workflow_input(input, &application, &posting)?;
            let mut request_seed = NewJobsWorkflowCommand {
                account_id: input.account_id.clone(),
                application_id: input.application_id.clone(),
                run_id: input.run_id.clone(),
                workflow_id: input.workflow_id.clone(),
                intervention_id: None,
                command_kind: JobsWorkflowCommandKind::Start,
                idempotency_key: input.idempotency_key.clone(),
                request: Value::Null,
                payload: JobsWorkflowCommandPayload::Start(JobsWorkflowStartMaterial {
                    workflow_input: input.workflow_input.clone(),
                    browser_session_id: input.browser_session.id.clone(),
                    result_request_id: String::new(),
                }),
                now_ms: input.now_ms,
            };
            request_seed.request = stage_workflow_command_request(&request_seed);
            let result_request_id = workflow_command_request_id_for_idempotency(&request_seed)?;
            let command = NewJobsWorkflowCommand {
                payload: stage_workflow_command_payload(
                    JobsWorkflowCommandKind::Start,
                    input.workflow_input.clone(),
                    input.browser_session.id.clone(),
                    result_request_id,
                    None,
                )?,
                ..request_seed
            };
            if let Some(existing) = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE account_id = ?1 AND command_kind = 'start'
                            AND idempotency_key_hmac_sha256 = ?2"
                    ),
                    params![
                        input.account_id,
                        workflow_command_idempotency_hmac(&command)?
                    ],
                    sqlite_workflow_command_row,
                )
                .optional()?
            {
                let replay = workflow_command_replay(
                    existing,
                    &command,
                    &workflow_command_request_hmac(&command)?,
                )?;
                require_managed_cloud_workflow_binding_replay_sqlite_tx(
                    &transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope.clone()),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            let hold_context = operational_hold_context_for_application_sqlite_tx(
                &transaction,
                &input.account_id,
                &input.application_id,
                Some("cloud"),
                None,
                None,
            )
            .map_err(anyhow::Error::new)?;
            require_operational_capability_sqlite_tx(
                &transaction,
                OperationalCapability::ApplicationQueue,
                &hold_context,
            )
            .map_err(anyhow::Error::new)?;
            stage_attempt_reservation_sqlite_tx(&transaction, input, &application, &posting)?;
            stage_packet_metering_sqlite_tx(&transaction, input, &application)?;
            stage_start_rows_sqlite_tx(&transaction, input, &mut application)?;
            let admission = admit_jobs_workflow_command_sqlite_tx(&transaction, &command, true)?;
            bind_managed_cloud_workflow_sqlite_tx(
                &transaction,
                &managed_cloud_binding_input(&admission, managed_cloud_scope),
            )?;
            transaction.commit()?;
            Ok(admission)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut transaction)
                .map_err(anyhow::Error::new)?;
            if let Some(existing) = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                      WHERE account_id = $1 AND command_kind = 'start'
                        AND idempotency_key_hmac_sha256 = $2 FOR SHARE"
                ),
                &[&input.account_id, &frozen_idempotency_hmac],
            )? {
                let replay = workflow_command_replay(
                    postgres_workflow_command_row(&existing),
                    &frozen_command,
                    &frozen_request_hmac,
                )?;
                let managed_cloud_scope =
                    crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                        .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
                require_managed_cloud_workflow_binding_replay_postgres_tx(
                    &mut transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            let managed_cloud_scope =
                crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                    .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
            lock_managed_cloud_workflow_admission_postgres_tx(
                &mut transaction,
                &managed_cloud_scope,
            )?;
            // The first replay lookup can race a concurrent first insert that
            // is still uncommitted. The managed-cloud prelock serializes that
            // insert, so repeat the immutable replay lookup immediately after
            // acquiring it and before consulting any mutable launch state.
            if let Some(existing) = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                      WHERE account_id = $1 AND command_kind = 'start'
                        AND idempotency_key_hmac_sha256 = $2 FOR SHARE"
                ),
                &[&input.account_id, &frozen_idempotency_hmac],
            )? {
                let replay = workflow_command_replay(
                    postgres_workflow_command_row(&existing),
                    &frozen_command,
                    &frozen_request_hmac,
                )?;
                require_managed_cloud_workflow_binding_replay_postgres_tx(
                    &mut transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope.clone()),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            lock_discovery_account_shared_postgres(&mut transaction, &input.account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut transaction,
                &input.account_id,
            )?;
            require_no_workflow_cleanup_postgres_tx(&mut transaction, &input.account_id)?;
            let entitlement = transaction.query_one(
                "SELECT cloud_browser FROM jobs_entitlements
                  WHERE account_id = $1 FOR UPDATE",
                &[&input.account_id],
            )?;
            if !entitlement.get::<_, bool>(0)
                || !cloud_distribution_ready_postgres_tx(&mut transaction)?
            {
                anyhow::bail!("cloud browser distribution is unavailable")
            }
            let (mut application, posting) = load_stage_application_postgres_tx(
                &mut transaction,
                &input.account_id,
                &input.application_id,
            )?;
            if !current_execution_authorized_postgres(
                &mut transaction,
                &input.account_id,
                &application,
                ExecutionAuthorityRunner::Cloud,
            )? {
                anyhow::bail!("current Jobs execution authority does not permit cloud queueing")
            }
            validate_stage_workflow_input(input, &application, &posting)?;
            let mut request_seed = NewJobsWorkflowCommand {
                account_id: input.account_id.clone(),
                application_id: input.application_id.clone(),
                run_id: input.run_id.clone(),
                workflow_id: input.workflow_id.clone(),
                intervention_id: None,
                command_kind: JobsWorkflowCommandKind::Start,
                idempotency_key: input.idempotency_key.clone(),
                request: Value::Null,
                payload: JobsWorkflowCommandPayload::Start(JobsWorkflowStartMaterial {
                    workflow_input: input.workflow_input.clone(),
                    browser_session_id: input.browser_session.id.clone(),
                    result_request_id: String::new(),
                }),
                now_ms: input.now_ms,
            };
            request_seed.request = stage_workflow_command_request(&request_seed);
            let result_request_id = workflow_command_request_id_for_idempotency(&request_seed)?;
            let command = NewJobsWorkflowCommand {
                payload: stage_workflow_command_payload(
                    JobsWorkflowCommandKind::Start,
                    input.workflow_input.clone(),
                    input.browser_session.id.clone(),
                    result_request_id,
                    None,
                )?,
                ..request_seed
            };
            let idempotency_hmac = workflow_command_idempotency_hmac(&command)?;
            if let Some(existing) = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                      WHERE account_id = $1 AND command_kind = 'start'
                        AND idempotency_key_hmac_sha256 = $2 FOR SHARE"
                ),
                &[&input.account_id, &idempotency_hmac],
            )? {
                let replay = workflow_command_replay(
                    postgres_workflow_command_row(&existing),
                    &command,
                    &workflow_command_request_hmac(&command)?,
                )?;
                require_managed_cloud_workflow_binding_replay_postgres_tx(
                    &mut transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope.clone()),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            let hold_context = operational_hold_context_for_application_postgres_tx(
                &mut transaction,
                &input.account_id,
                &input.application_id,
                Some("cloud"),
                None,
                None,
            )
            .map_err(anyhow::Error::new)?;
            require_operational_capability_postgres_tx(
                &mut transaction,
                OperationalCapability::ApplicationQueue,
                &hold_context,
            )
            .map_err(anyhow::Error::new)?;
            stage_attempt_reservation_postgres_tx(&mut transaction, input, &application, &posting)?;
            stage_packet_metering_postgres_tx(&mut transaction, input, &application)?;
            stage_start_rows_postgres_tx(&mut transaction, input, &mut application)?;
            let admission =
                admit_jobs_workflow_command_postgres_tx(&mut transaction, &command, true)?;
            bind_managed_cloud_workflow_postgres_tx(
                &mut transaction,
                &managed_cloud_binding_input(&admission, managed_cloud_scope),
            )?;
            transaction.commit()?;
            Ok(admission)
        }
    })
}

fn managed_cloud_binding_input(
    admission: &JobsWorkflowCommandAdmission,
    scope: ManagedCloudScope,
) -> ManagedCloudWorkflowBindingInput {
    ManagedCloudWorkflowBindingInput {
        command_id: admission.command.id.clone(),
        account_id: admission.command.account_id.clone(),
        application_id: admission.command.application_id.clone(),
        run_id: admission.command.run_id.clone(),
        workflow_id: admission.command.workflow_id.clone(),
        scope,
    }
}

fn validate_resume_start_material(
    start: &JobsWorkflowCommand,
    input: &StageCloudWorkflowResume,
) -> Result<()> {
    let JobsWorkflowCommandPayload::Start(material) = &start.envelope.payload else {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    };
    if start.account_id != input.account_id
        || start.application_id != input.application_id
        || start.run_id != input.run_id
        || start.workflow_id != input.workflow_id
        || start.first_request_started_at_ms.is_none()
        || !matches!(
            start.state,
            JobsWorkflowCommandState::Delivering
                | JobsWorkflowCommandState::DeliveryUnknown
                | JobsWorkflowCommandState::Accepted
        )
        || material.browser_session_id != input.browser_session_id
        || material.result_request_id != start.request_id
        || canonical_workflow_command_value(&material.workflow_input)?
            != canonical_workflow_command_value(&input.workflow_input)?
    {
        anyhow::bail!("workflow resume does not match the started workflow authority")
    }
    Ok(())
}

fn provider_final_review_intervention(intervention: &Intervention, posting: &JobPosting) -> bool {
    if intervention.kind != "browser_takeover"
        || intervention.resolution_kind != "browser_takeover"
        || !intervention.resume_after_resolution
        || !intervention.choices.is_empty()
        || intervention
            .metadata
            .get("_bluey_worker_receipt_v1")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return false;
    }
    let Some(receipt) = intervention.metadata.get("receipt") else {
        return false;
    };
    let Some(source) = receipt.get("intervention").and_then(Value::as_object) else {
        return false;
    };
    let Some(resolution) = source.get("resolution").and_then(Value::as_object) else {
        return false;
    };
    if receipt.get("status").and_then(Value::as_str) != Some("needs_input")
        || !receipt
            .get("issues")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        || source.get("kind").and_then(Value::as_str) != Some("browser_takeover")
        || source
            .get("takeoverUrl")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || resolution.get("kind").and_then(Value::as_str) != Some("browser_takeover")
        || resolution.get("resumeAfter").and_then(Value::as_bool) != Some(true)
        || source.get("title").and_then(Value::as_str) != Some(intervention.title.as_str())
        || source.get("detail").and_then(Value::as_str) != Some(intervention.detail.as_str())
    {
        return false;
    }
    let provider = reqwest::Url::parse(&posting.canonical_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase));
    match provider.as_deref() {
        Some("boards.greenhouse.io" | "job-boards.greenhouse.io") => {
            intervention.title == "Review the Greenhouse application"
                && intervention.detail
                    == "Review every employer-facing field and document in the preserved form, then approve submission."
        }
        Some("jobs.lever.co" | "jobs.eu.lever.co") => {
            intervention.title == "Review this Lever application"
                && intervention.detail
                    == "Review every answer and attachment in the preserved browser. Bluey will not submit until you explicitly approve final review."
        }
        _ => false,
    }
}

fn validate_resume_resolution(
    resolution: &Value,
    intervention: &Intervention,
    posting: &JobPosting,
    now_ms: i64,
) -> Result<()> {
    let fields = resolution
        .as_object()
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if fields.is_empty()
        || fields.len() > 3
        || fields
            .keys()
            .any(|key| !matches!(key.as_str(), "action" | "field" | "answer"))
        || fields
            .get("field")
            .is_some_and(|value| value.as_str().is_none_or(|text| !text.trim().is_empty()))
        || fields
            .get("answer")
            .is_some_and(|value| value.as_str().is_none_or(|text| !text.trim().is_empty()))
    {
        anyhow::bail!("workflow resume may not carry application answer material")
    }
    let action = fields
        .get("action")
        .and_then(Value::as_str)
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    let authority_matches = match action {
        "approve_email_otp" => {
            intervention.kind == "two_factor"
                && intervention.resolution_kind == "email_otp_approval"
                && intervention.resume_after_resolution
                && matches!(intervention.provider.as_str(), "gmail" | "outlook_email")
                && !intervention.provider_message_id.trim().is_empty()
                && intervention
                    .expires_at_ms
                    .is_some_and(|expires_at| expires_at > now_ms)
        }
        "approve_submission" => provider_final_review_intervention(intervention, posting),
        "resume_browser_takeover" | "browser_takeover_complete" => {
            intervention.kind == "browser_takeover"
                && intervention.resolution_kind == "browser_takeover"
                && intervention.resume_after_resolution
        }
        _ => false,
    };
    if !authority_matches {
        anyhow::bail!("workflow resume action does not match the open intervention")
    }
    Ok(())
}

fn approve_workflow_intervention(intervention: &mut Intervention, now_ms: i64) -> Result<()> {
    intervention.status = "approved".to_string();
    let metadata = intervention
        .metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("workflow intervention metadata is invalid"))?;
    metadata.insert("approved_at_ms".to_string(), json!(now_ms));
    Ok(())
}

fn workflow_intervention_replay_matches(
    expected: &Intervention,
    actual: &Intervention,
    command: &StoredWorkflowCommandRow,
) -> Result<bool> {
    let expected_receipt_hmac = workflow_command_hmac(
        "intervention-source",
        expected
            .metadata
            .get("receipt")
            .ok_or(JobsWorkflowCommandError::IdentityConflict)?,
        WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
    )?;
    let actual_receipt_hmac = workflow_command_hmac(
        "intervention-source",
        actual
            .metadata
            .get("receipt")
            .ok_or(JobsWorkflowCommandError::IdentityConflict)?,
        WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
    )?;
    Ok(actual.id == expected.id
        && actual.application_id.as_deref() == Some(command.application_id.as_str())
        && actual.kind == expected.kind
        && actual.title == expected.title
        && actual.detail == expected.detail
        && actual.choices == expected.choices
        && actual.resolution_kind == expected.resolution_kind
        && actual.resume_after_resolution == expected.resume_after_resolution
        && actual.provider == expected.provider
        && actual.provider_message_id == expected.provider_message_id
        && actual.expires_at_ms == expected.expires_at_ms
        && actual.created_at_ms == expected.created_at_ms
        && workflow_command_hmac_matches(&actual_receipt_hmac, &expected_receipt_hmac))
}

fn intervention_from_workflow_receipt(
    receipt: &Value,
    intervention_id: &str,
) -> Result<Intervention> {
    let receipt_fields = receipt
        .as_object()
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if receipt_fields.keys().any(|key| {
        !matches!(
            key.as_str(),
            "status"
                | "issues"
                | "intervention"
                | "submitHttpStatus"
                | "confirmationText"
                | "confirmationUrl"
                | "submittedAt"
        )
    }) || receipt.get("status").and_then(Value::as_str) != Some("needs_input")
        || receipt
            .get("issues")
            .and_then(Value::as_array)
            .is_none_or(|issues| issues.len() > 100)
        || contains_authentication_secret(receipt)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let source = receipt
        .get("intervention")
        .and_then(Value::as_object)
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    if source.keys().any(|key| {
        !matches!(
            key.as_str(),
            "kind" | "title" | "detail" | "field" | "choices" | "takeoverUrl" | "resolution"
        )
    }) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let kind = source
        .get("kind")
        .and_then(Value::as_str)
        .filter(|kind| {
            matches!(
                *kind,
                "captcha"
                    | "two_factor"
                    | "assessment"
                    | "unknown_question"
                    | "missing_fact"
                    | "sensitive_question"
                    | "browser_takeover"
            )
        })
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    let title = source
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= 1_000)
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    let detail = source
        .get("detail")
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 8_000)
        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
    let choices = source
        .get("choices")
        .map(|value| {
            value
                .as_array()
                .filter(|values| values.len() <= 100)
                .ok_or(JobsWorkflowCommandError::InvalidRequest)?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .filter(|text| text.len() <= 1_000)
                        .map(str::to_string)
                        .ok_or(JobsWorkflowCommandError::InvalidRequest)
                })
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let resolution = source.get("resolution").and_then(Value::as_object);
    if resolution.is_some_and(|value| {
        value.keys().any(|key| {
            !matches!(
                key.as_str(),
                "kind" | "resumeAfter" | "expiresAt" | "provider" | "messageId"
            )
        })
    }) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let resolution_kind = resolution
        .and_then(|value| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("browser_takeover");
    if !matches!(
        resolution_kind,
        "browser_takeover" | "email_otp_approval" | "answer"
    ) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let resume_after_resolution = resolution
        .and_then(|value| value.get("resumeAfter"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let provider = resolution
        .and_then(|value| value.get("provider"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let provider_message_id = resolution
        .and_then(|value| value.get("messageId"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expires_at_ms = resolution
        .and_then(|value| value.get("expiresAt"))
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(value) => chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|parsed| parsed.timestamp_millis()),
            _ => None,
        });
    if resolution_kind == "email_otp_approval"
        && (kind != "two_factor"
            || !matches!(provider, "gmail" | "outlook_email")
            || provider_message_id.trim().is_empty()
            || expires_at_ms.is_none())
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    Ok(Intervention {
        id: intervention_id.to_string(),
        application_id: None,
        kind: kind.to_string(),
        status: "open".to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        choices,
        resolution_kind: resolution_kind.to_string(),
        resume_after_resolution,
        provider: provider.to_string(),
        provider_message_id: provider_message_id.to_string(),
        expires_at_ms,
        metadata: json!({
            "receipt": receipt,
            "_bluey_worker_receipt_v1": true,
        }),
        created_at_ms: 0,
        resolved_at_ms: None,
    })
}

fn validate_external_workflow_authority(
    command: &StoredWorkflowCommandRow,
    request_id: &str,
    payload_hmac_sha256: &str,
    workflow_id: &str,
    command_kind: JobsWorkflowCommandKind,
    intervention_id: Option<&str>,
) -> Result<()> {
    if !workflow_command_opaque_identifier(request_id, 128)
        || !workflow_command_opaque_identifier(workflow_id, 192)
        || !workflow_command_digest(payload_hmac_sha256)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    if command.request_id != request_id
        || command.workflow_id != workflow_id
        || command.command_kind != command_kind.as_str()
        || command.intervention_id.as_deref() != intervention_id
        || !workflow_command_hmac_matches(&command.payload_hmac_sha256, payload_hmac_sha256)
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    if command.first_request_started_at_ms.is_none() {
        return Err(JobsWorkflowCommandError::RequestNotStarted.into());
    }
    if !matches!(
        command.state.as_str(),
        "delivering" | "delivery_unknown" | "accepted"
    ) {
        return Err(JobsWorkflowCommandError::InvalidState.into());
    }
    Ok(())
}

fn deterministic_workflow_intervention_id(command: &StoredWorkflowCommandRow) -> Result<String> {
    let digest = workflow_command_hmac(
        "intervention-id",
        &json!({
            "requestId": command.request_id,
            "payloadHmacSha256": command.payload_hmac_sha256,
            "workflowId": command.workflow_id,
        }),
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
    )?;
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hex::decode(digest)?[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!("wfint-v2-{}", uuid::Uuid::from_bytes(bytes)))
}

fn workflow_command_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub fn stage_cloud_workflow_resume(
    pool: &DbPool,
    input: &StageCloudWorkflowResume,
) -> Result<JobsWorkflowCommandAdmission> {
    if !workflow_command_identifier(&input.browser_session_id, 128)
        || !workflow_command_identifier(&input.intervention_id, 128)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let mut request_seed = NewJobsWorkflowCommand {
        account_id: input.account_id.clone(),
        application_id: input.application_id.clone(),
        run_id: input.run_id.clone(),
        workflow_id: input.workflow_id.clone(),
        intervention_id: Some(input.intervention_id.clone()),
        command_kind: JobsWorkflowCommandKind::Resume,
        idempotency_key: input.idempotency_key.clone(),
        request: Value::Null,
        payload: JobsWorkflowCommandPayload::Resume(JobsWorkflowResumeMaterial {
            workflow_input: input.workflow_input.clone(),
            browser_session_id: input.browser_session_id.clone(),
            result_request_id: String::new(),
            resolution: input.resolution.clone(),
        }),
        now_ms: input.now_ms,
    };
    request_seed.request = stage_workflow_command_request(&request_seed);
    let result_request_id = workflow_command_request_id_for_idempotency(&request_seed)?;
    let command = NewJobsWorkflowCommand {
        payload: stage_workflow_command_payload(
            JobsWorkflowCommandKind::Resume,
            input.workflow_input.clone(),
            input.browser_session_id.clone(),
            result_request_id,
            Some(input.resolution.clone()),
        )?,
        ..request_seed
    };
    validate_new_workflow_command(&command)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let idempotency_hmac = workflow_command_idempotency_hmac(&command)?;
            if let Some(existing) = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE account_id = ?1 AND command_kind = 'resume'
                            AND idempotency_key_hmac_sha256 = ?2"
                    ),
                    params![input.account_id, idempotency_hmac],
                    sqlite_workflow_command_row,
                )
                .optional()?
            {
                let replay = workflow_command_replay(
                    existing,
                    &command,
                    &workflow_command_request_hmac(&command)?,
                )?;
                let managed_cloud_scope =
                    crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                        .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
                require_managed_cloud_workflow_binding_replay_sqlite_tx(
                    &transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            let managed_cloud_scope =
                crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                    .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &transaction,
                &input.account_id,
            )?;
            require_no_workflow_cleanup_sqlite_tx(&transaction, &input.account_id)?;
            let cloud_browser: i64 = transaction.query_row(
                "SELECT cloud_browser FROM jobs_entitlements WHERE account_id = ?1",
                params![input.account_id],
                |row| row.get(0),
            )?;
            if cloud_browser == 0 {
                anyhow::bail!("cloud browser distribution is unavailable")
            }
            let (application, posting) = load_stage_application_sqlite_tx(
                &transaction,
                &input.account_id,
                &input.application_id,
            )?;
            if application.state != "needs_input"
                || application.run_id.as_deref() != Some(input.run_id.as_str())
            {
                anyhow::bail!("application is not waiting for this workflow intervention")
            }
            let start_row = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                     WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                       AND workflow_id = ?4 AND command_kind = 'start'
                       AND state IN ('delivering', 'delivery_unknown', 'accepted')
                       AND first_request_started_at_ms IS NOT NULL"
                    ),
                    params![
                        input.account_id,
                        input.application_id,
                        input.run_id,
                        input.workflow_id,
                    ],
                    sqlite_workflow_command_row,
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("workflow start has no request-start authority"))?;
            let start = workflow_command_from_stored(start_row)?;
            validate_resume_start_material(&start, input)?;
            let intervention_json: String = transaction
                .query_row(
                    "SELECT intervention_json FROM jobs_interventions
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND status = 'open'",
                    params![
                        input.intervention_id,
                        input.account_id,
                        input.application_id,
                    ],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("intervention is no longer open"))?;
            let mut intervention: Intervention =
                parse_json(intervention_json, "staged Jobs intervention")?;
            validate_resume_resolution(&input.resolution, &intervention, &posting, input.now_ms)?;
            approve_workflow_intervention(&mut intervention, input.now_ms)?;
            let encoded = to_json(&intervention, "staged Jobs intervention")?;
            let changed = transaction.execute(
                "UPDATE jobs_interventions
                    SET status = 'approved', intervention_json = ?1
                  WHERE id = ?2 AND account_id = ?3 AND application_id = ?4
                    AND status = 'open'",
                params![
                    encoded,
                    input.intervention_id,
                    input.account_id,
                    input.application_id,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("intervention changed before workflow resume admission")
            }
            let admission = admit_jobs_workflow_command_sqlite_tx(&transaction, &command, true)?;
            bind_managed_cloud_workflow_sqlite_tx(
                &transaction,
                &managed_cloud_binding_input(&admission, managed_cloud_scope),
            )?;
            transaction.commit()?;
            Ok(admission)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut transaction)
                .map_err(anyhow::Error::new)?;
            let idempotency_hmac = workflow_command_idempotency_hmac(&command)?;
            if let Some(existing) = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                      WHERE account_id = $1 AND command_kind = 'resume'
                        AND idempotency_key_hmac_sha256 = $2 FOR SHARE"
                ),
                &[&input.account_id, &idempotency_hmac],
            )? {
                let replay = workflow_command_replay(
                    postgres_workflow_command_row(&existing),
                    &command,
                    &workflow_command_request_hmac(&command)?,
                )?;
                let managed_cloud_scope =
                    crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                        .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
                require_managed_cloud_workflow_binding_replay_postgres_tx(
                    &mut transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            let managed_cloud_scope =
                crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission()
                    .ok_or_else(|| anyhow::anyhow!("managed cloud admission is unavailable"))?;
            lock_managed_cloud_workflow_admission_postgres_tx(
                &mut transaction,
                &managed_cloud_scope,
            )?;
            // See the start path: the prelock is the first point at which a
            // concurrent first insert is guaranteed visible. Preserve exact
            // replay before account, entitlement, intervention, or readiness
            // state can reject it.
            if let Some(existing) = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                      WHERE account_id = $1 AND command_kind = 'resume'
                        AND idempotency_key_hmac_sha256 = $2 FOR SHARE"
                ),
                &[&input.account_id, &idempotency_hmac],
            )? {
                let replay = workflow_command_replay(
                    postgres_workflow_command_row(&existing),
                    &command,
                    &workflow_command_request_hmac(&command)?,
                )?;
                require_managed_cloud_workflow_binding_replay_postgres_tx(
                    &mut transaction,
                    &managed_cloud_binding_input(&replay, managed_cloud_scope.clone()),
                )?;
                transaction.commit()?;
                return Ok(replay);
            }
            lock_discovery_account_shared_postgres(&mut transaction, &input.account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut transaction,
                &input.account_id,
            )?;
            require_no_workflow_cleanup_postgres_tx(&mut transaction, &input.account_id)?;
            let entitlement = transaction.query_one(
                "SELECT cloud_browser FROM jobs_entitlements
                  WHERE account_id = $1 FOR UPDATE",
                &[&input.account_id],
            )?;
            if !entitlement.get::<_, bool>(0) {
                anyhow::bail!("cloud browser distribution is unavailable")
            }
            let (application, posting) = load_stage_application_postgres_tx(
                &mut transaction,
                &input.account_id,
                &input.application_id,
            )?;
            if application.state != "needs_input"
                || application.run_id.as_deref() != Some(input.run_id.as_str())
            {
                anyhow::bail!("application is not waiting for this workflow intervention")
            }
            let start_row = transaction
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                         WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                           AND workflow_id = $4 AND command_kind = 'start'
                           AND state IN ('delivering', 'delivery_unknown', 'accepted')
                           AND first_request_started_at_ms IS NOT NULL FOR SHARE"
                    ),
                    &[
                        &input.account_id,
                        &input.application_id,
                        &input.run_id,
                        &input.workflow_id,
                    ],
                )?
                .ok_or_else(|| anyhow::anyhow!("workflow start has no request-start authority"))?;
            let start = workflow_command_from_stored(postgres_workflow_command_row(&start_row))?;
            validate_resume_start_material(&start, input)?;
            let row = transaction
                .query_opt(
                    "SELECT intervention_json FROM jobs_interventions
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND status = 'open' FOR UPDATE",
                    &[
                        &input.intervention_id,
                        &input.account_id,
                        &input.application_id,
                    ],
                )?
                .ok_or_else(|| anyhow::anyhow!("intervention is no longer open"))?;
            let mut intervention: Intervention =
                parse_json(row.get(0), "staged Jobs intervention")?;
            validate_resume_resolution(&input.resolution, &intervention, &posting, input.now_ms)?;
            approve_workflow_intervention(&mut intervention, input.now_ms)?;
            let encoded = to_json(&intervention, "staged Jobs intervention")?;
            let changed = transaction.execute(
                "UPDATE jobs_interventions
                    SET status = 'approved', intervention_json = $1
                  WHERE id = $2 AND account_id = $3 AND application_id = $4
                    AND status = 'open'",
                &[
                    &encoded,
                    &input.intervention_id,
                    &input.account_id,
                    &input.application_id,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("intervention changed before workflow resume admission")
            }
            let admission =
                admit_jobs_workflow_command_postgres_tx(&mut transaction, &command, true)?;
            bind_managed_cloud_workflow_postgres_tx(
                &mut transaction,
                &managed_cloud_binding_input(&admission, managed_cloud_scope),
            )?;
            transaction.commit()?;
            Ok(admission)
        }
    })
}

fn workflow_command_random_lease_token() -> String {
    let mut bytes = [0_u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn workflow_command_lease_token_sha256(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn validate_workflow_command_lease_input(owner_id: &str, now_ms: i64, lease_ms: i64) -> Result<()> {
    if !workflow_command_identifier(owner_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms)
        || !(WORKFLOW_COMMAND_LEASE_MIN_MS..=WORKFLOW_COMMAND_LEASE_MAX_MS).contains(&lease_ms)
        || now_ms
            .checked_add(lease_ms)
            .is_none_or(|value| value > WORKFLOW_COMMAND_SAFE_INTEGER_MAX)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    Ok(())
}

fn validate_workflow_command_lease(
    stored: &StoredWorkflowCommandRow,
    lease: &JobsWorkflowCommandLease,
    required_state: JobsWorkflowCommandState,
    now_ms: i64,
) -> Result<()> {
    let token_sha256 = workflow_command_lease_token_sha256(&lease.lease_token);
    if stored.id != lease.command.id
        || stored.account_id != lease.command.account_id
        || stored.request_id != lease.command.request_id
        || !workflow_command_hmac_matches(
            &stored.payload_hmac_sha256,
            &lease.command.payload_hmac_sha256,
        )
        || stored.active_attempt_id.as_deref() != Some(lease.attempt_id.as_str())
        || stored.fence != lease.fence
        || stored.state
            != match required_state {
                JobsWorkflowCommandState::Claimed => "claimed",
                JobsWorkflowCommandState::Delivering => "delivering",
                _ => return Err(JobsWorkflowCommandError::InvalidState.into()),
            }
        || stored.lease_owner.as_deref() != Some(lease.lease_owner.as_str())
        || stored
            .lease_token_sha256
            .as_deref()
            .is_none_or(|stored_hash| !workflow_command_hmac_matches(stored_hash, &token_sha256))
        || stored.lease_expires_at_ms != Some(lease.lease_expires_at_ms)
    {
        return Err(JobsWorkflowCommandError::StaleLease.into());
    }
    if stored
        .lease_expires_at_ms
        .is_none_or(|expires_at| expires_at <= now_ms)
    {
        return Err(JobsWorkflowCommandError::LeaseExpired.into());
    }
    Ok(())
}

fn workflow_command_retry_at(now_ms: i64, requested: Option<i64>) -> Result<i64> {
    let retry_at = requested.unwrap_or(now_ms);
    if retry_at < now_ms
        || retry_at
            > now_ms
                .saturating_add(WORKFLOW_COMMAND_RETRY_MAX_MS)
                .min(WORKFLOW_COMMAND_SAFE_INTEGER_MAX)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    Ok(retry_at)
}

fn workflow_command_event(
    completion: &JobsWorkflowCommandCompletion,
) -> (&'static str, Option<&'static str>, Option<&str>) {
    match completion {
        JobsWorkflowCommandCompletion::Accepted(receipt) => (
            receipt.outcome.event_kind(),
            None,
            Some(receipt.temporal_run_id.as_str()),
        ),
        JobsWorkflowCommandCompletion::DeliveryUnknown(reason) => {
            ("delivery_unknown", Some(reason.as_str()), None)
        }
        JobsWorkflowCommandCompletion::IdentityConflict => {
            ("identity_conflict", Some("identity_conflict"), None)
        }
        JobsWorkflowCommandCompletion::Rejected(reason) => {
            ("rejected", Some(reason.as_str()), None)
        }
    }
}

fn validate_workflow_command_completion(
    command: &StoredWorkflowCommandRow,
    completion: &JobsWorkflowCommandCompletion,
    retry_at_ms: Option<i64>,
    now_ms: i64,
) -> Result<Option<i64>> {
    if let JobsWorkflowCommandCompletion::Accepted(receipt) = completion {
        if !workflow_command_opaque_identifier(&receipt.request_id, 128)
            || !workflow_command_opaque_identifier(&receipt.workflow_id, 192)
            || !workflow_command_opaque_identifier(&receipt.temporal_run_id, 128)
            || receipt.request_id != command.request_id
            || receipt.workflow_id != command.workflow_id
            || receipt.intervention_id != command.intervention_id
            || !workflow_command_hmac_matches(
                &receipt.payload_hmac_sha256,
                &command.payload_hmac_sha256,
            )
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
    }
    match completion {
        JobsWorkflowCommandCompletion::DeliveryUnknown(_) => {
            Ok(Some(workflow_command_retry_at(now_ms, retry_at_ms)?))
        }
        _ if retry_at_ms.is_some() => Err(JobsWorkflowCommandError::InvalidRequest.into()),
        _ => Ok(None),
    }
}

fn bind_workflow_execution_acceptance_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    receipt: &JobsWorkflowAcceptanceReceipt,
    now_ms: i64,
) -> Result<()> {
    let finalized_at_ms: Option<i64> = tx
        .query_row(
            "SELECT finalization.finalized_at_ms
               FROM jobs_workflow_execution_finalizations finalization
               JOIN jobs_workflow_commands final_command
                 ON final_command.id = finalization.command_id
              WHERE final_command.account_id = ?1
                AND final_command.workflow_id = ?2
              ORDER BY finalization.finalized_at_ms DESC LIMIT 1",
            params![command.account_id, command.workflow_id],
            |row| row.get(0),
        )
        .optional()?;
    let application_submitted: bool = tx
        .query_row(
            "SELECT application_json, state, job_id FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![command.account_id, command.application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .map(|(raw, state, job_id)| {
            let application = parse_application_json(
                raw,
                &command.application_id,
                &job_id,
                "workflow acceptance application",
            )?;
            Ok::<_, anyhow::Error>(
                state == "submitted"
                    && application.state == state
                    && application.run_id.as_deref() == Some(command.run_id.as_str()),
            )
        })
        .transpose()?
        .unwrap_or(false);
    let terminal_at_ms = finalized_at_ms.or(application_submitted.then_some(now_ms));
    if command.command_kind == "start" {
        tx.execute(
            "INSERT INTO jobs_workflow_executions (
                account_id, workflow_id, start_command_id, first_execution_run_id,
                lifecycle_state, cleanup_state, terminal_at_ms, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'required', ?6, ?7, ?7)
             ON CONFLICT(account_id, workflow_id) DO NOTHING",
            params![
                command.account_id,
                command.workflow_id,
                command.id,
                receipt.temporal_run_id,
                if terminal_at_ms.is_some() {
                    "terminal"
                } else {
                    "accepted"
                },
                terminal_at_ms,
                now_ms,
            ],
        )?;
    }
    let authority: Option<(String, String)> = tx
        .query_row(
            "SELECT start_command_id, first_execution_run_id
               FROM jobs_workflow_executions
              WHERE account_id = ?1 AND workflow_id = ?2",
            params![command.account_id, command.workflow_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((start_command_id, first_execution_run_id)) = authority else {
        anyhow::bail!("workflow execution acceptance has no start authority")
    };
    if (command.command_kind == "start" && start_command_id != command.id)
        || first_execution_run_id != receipt.temporal_run_id
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    if command.command_kind == "start" {
        let cleanup_target: Option<(i64, String, Option<String>, String)> = tx
            .query_row(
                "SELECT generation, target_set_hmac_sha256, first_execution_run_id,
                        target_state
                   FROM jobs_workflow_cleanup_targets
                  WHERE account_id = ?1 AND workflow_id = ?2 AND start_command_id = ?3",
                params![command.account_id, command.workflow_id, command.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        if let Some((generation, target_digest, target_run_id, target_state)) = cleanup_target {
            if target_run_id
                .as_deref()
                .is_some_and(|value| value != receipt.temporal_run_id)
                || !matches!(
                    target_state.as_str(),
                    "delivery_drain" | "identity_reconcile" | "cleanup_required"
                )
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if target_run_id.is_none()
                && tx.execute(
                    "UPDATE jobs_workflow_cleanup_targets
                        SET first_execution_run_id = ?1, target_state = 'cleanup_required',
                            updated_at_ms = MAX(?2, updated_at_ms + 1)
                      WHERE account_id = ?3 AND generation = ?4 AND workflow_id = ?5
                        AND target_state IN ('delivery_drain', 'identity_reconcile')
                        AND first_execution_run_id IS NULL",
                    params![
                        receipt.temporal_run_id,
                        now_ms,
                        command.account_id,
                        generation,
                        command.workflow_id
                    ],
                )? != 1
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            tx.execute(
                "UPDATE jobs_workflow_executions
                    SET deletion_target_generation = ?1, deletion_target_hmac_sha256 = ?2,
                        updated_at_ms = MAX(?3, updated_at_ms + 1)
                  WHERE account_id = ?4 AND workflow_id = ?5
                    AND (deletion_target_generation IS NULL
                      OR (deletion_target_generation = ?1
                        AND deletion_target_hmac_sha256 = ?2))",
                params![
                    generation,
                    target_digest,
                    now_ms,
                    command.account_id,
                    command.workflow_id
                ],
            )?;
        }
    }
    Ok(())
}

fn bind_workflow_execution_acceptance_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    receipt: &JobsWorkflowAcceptanceReceipt,
    now_ms: i64,
) -> Result<()> {
    let finalized_at_ms = tx
        .query_opt(
            "SELECT finalization.finalized_at_ms
               FROM jobs_workflow_execution_finalizations finalization
               JOIN jobs_workflow_commands final_command
                 ON final_command.id = finalization.command_id
              WHERE final_command.account_id = $1
                AND final_command.workflow_id = $2
              ORDER BY finalization.finalized_at_ms DESC LIMIT 1",
            &[&command.account_id, &command.workflow_id],
        )?
        .map(|row| row.get::<_, i64>(0));
    let application_submitted = tx
        .query_opt(
            "SELECT application_json, state, job_id FROM jobs_applications
              WHERE account_id = $1 AND id = $2",
            &[&command.account_id, &command.application_id],
        )?
        .map(|row| {
            let state: String = row.get(1);
            let job_id: String = row.get(2);
            let application = parse_application_json(
                row.get(0),
                &command.application_id,
                &job_id,
                "workflow acceptance application",
            )?;
            Ok::<_, anyhow::Error>(
                state == "submitted"
                    && application.state == state
                    && application.run_id.as_deref() == Some(command.run_id.as_str()),
            )
        })
        .transpose()?
        .unwrap_or(false);
    let terminal_at_ms = finalized_at_ms.or(application_submitted.then_some(now_ms));
    if command.command_kind == "start" {
        tx.execute(
            "INSERT INTO jobs_workflow_executions (
                account_id, workflow_id, start_command_id, first_execution_run_id,
                lifecycle_state, cleanup_state, terminal_at_ms, created_at_ms, updated_at_ms
             ) VALUES ($1, $2, $3, $4, $5, 'required', $6, $7, $7)
             ON CONFLICT(account_id, workflow_id) DO NOTHING",
            &[
                &command.account_id,
                &command.workflow_id,
                &command.id,
                &receipt.temporal_run_id,
                &if terminal_at_ms.is_some() {
                    "terminal"
                } else {
                    "accepted"
                },
                &terminal_at_ms,
                &now_ms,
            ],
        )?;
    }
    let authority = tx.query_opt(
        "SELECT start_command_id, first_execution_run_id
           FROM jobs_workflow_executions
          WHERE account_id = $1 AND workflow_id = $2 FOR SHARE",
        &[&command.account_id, &command.workflow_id],
    )?;
    let Some(authority) = authority else {
        anyhow::bail!("workflow execution acceptance has no start authority")
    };
    if (command.command_kind == "start" && authority.get::<_, String>(0) != command.id)
        || authority.get::<_, String>(1) != receipt.temporal_run_id
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    if command.command_kind == "start" {
        if let Some(target) = tx.query_opt(
            "SELECT generation, target_set_hmac_sha256, first_execution_run_id, target_state
               FROM jobs_workflow_cleanup_targets
              WHERE account_id = $1 AND workflow_id = $2 AND start_command_id = $3
              FOR UPDATE",
            &[&command.account_id, &command.workflow_id, &command.id],
        )? {
            let generation: i64 = target.get(0);
            let target_digest: String = target.get(1);
            let target_run_id: Option<String> = target.get(2);
            let target_state: String = target.get(3);
            if target_run_id
                .as_deref()
                .is_some_and(|value| value != receipt.temporal_run_id)
                || !matches!(
                    target_state.as_str(),
                    "delivery_drain" | "identity_reconcile" | "cleanup_required"
                )
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if target_run_id.is_none()
                && tx.execute(
                    "UPDATE jobs_workflow_cleanup_targets
                        SET first_execution_run_id = $1, target_state = 'cleanup_required',
                            updated_at_ms = GREATEST($2, updated_at_ms + 1)
                      WHERE account_id = $3 AND generation = $4 AND workflow_id = $5
                        AND target_state IN ('delivery_drain', 'identity_reconcile')
                        AND first_execution_run_id IS NULL",
                    &[
                        &receipt.temporal_run_id,
                        &now_ms,
                        &command.account_id,
                        &generation,
                        &command.workflow_id,
                    ],
                )? != 1
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            tx.execute(
                "UPDATE jobs_workflow_executions
                    SET deletion_target_generation = $1, deletion_target_hmac_sha256 = $2,
                        updated_at_ms = GREATEST($3, updated_at_ms + 1)
                  WHERE account_id = $4 AND workflow_id = $5
                    AND (deletion_target_generation IS NULL
                      OR (deletion_target_generation = $1
                        AND deletion_target_hmac_sha256 = $2))",
                &[
                    &generation,
                    &target_digest,
                    &now_ms,
                    &command.account_id,
                    &command.workflow_id,
                ],
            )?;
        }
    }
    Ok(())
}

fn terminalize_workflow_rows_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    state: &str,
    now_ms: i64,
) -> Result<()> {
    let row: Option<(String, String, String)> = tx
        .query_row(
            SQLITE_WORKFLOW_APPLICATION_AUTHORITY_SQL,
            params![command.account_id, command.application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((raw, relational_state, job_id)) = row {
        let mut application = parse_application_json(
            raw,
            &command.application_id,
            &job_id,
            "workflow terminal application",
        )?;
        if application.run_id.as_deref() == Some(command.run_id.as_str())
            && application.state == relational_state
            && matches!(
                application.state.as_str(),
                "queued" | "running" | "needs_input"
            )
        {
            application.state = state.to_string();
            application.updated_at_ms = now_ms.max(application.updated_at_ms.saturating_add(1));
            let encoded = to_json(&application, "workflow terminal application")?;
            tx.execute(
                "UPDATE jobs_applications SET state = ?1, application_json = ?2,
                    updated_at_ms = ?3 WHERE account_id = ?4 AND id = ?5 AND state = ?6",
                params![
                    state,
                    encoded,
                    application.updated_at_ms,
                    command.account_id,
                    command.application_id,
                    relational_state,
                ],
            )?;
        }
    }
    let session_id = format!("cloud-{}", command.application_id);
    if let Some(raw) = tx
        .query_row(
            "SELECT session_json FROM jobs_browser_sessions
              WHERE id = ?1 AND account_id = ?2 AND runner = 'cloud'",
            params![session_id, command.account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        let mut session: BrowserSession = parse_json(raw, "workflow terminal browser session")?;
        if session.application_id.as_deref() == Some(command.application_id.as_str())
            && matches!(
                session.status.as_str(),
                "queued" | "running" | "needs_input"
            )
        {
            session.status = state.to_string();
            session.updated_at_ms = now_ms.max(session.updated_at_ms.saturating_add(1));
            let encoded = to_json(&session, "workflow terminal browser session")?;
            tx.execute(
                "UPDATE jobs_browser_sessions SET status = ?1, session_json = ?2,
                    updated_at_ms = ?3 WHERE id = ?4 AND account_id = ?5",
                params![
                    state,
                    encoded,
                    session.updated_at_ms,
                    session_id,
                    command.account_id
                ],
            )?;
        }
    }
    tx.execute(
        "UPDATE jobs_attempt_reservations SET status = ?1, updated_at_ms = ?2
          WHERE account_id = ?3 AND application_id = ?4
            AND runner = 'cloud' AND status IN ('reserved', 'running')",
        params![state, now_ms, command.account_id, command.application_id],
    )?;
    Ok(())
}

fn terminalize_workflow_rows_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    state: &str,
    now_ms: i64,
) -> Result<()> {
    let row = tx.query_opt(
        POSTGRES_WORKFLOW_APPLICATION_AUTHORITY_SQL,
        &[&command.account_id, &command.application_id],
    )?;
    if let Some(row) = row {
        let relational_state: String = row.get(1);
        let mut application = parse_application_json(
            row.get(0),
            &command.application_id,
            row.get(2),
            "workflow terminal application",
        )?;
        if application.run_id.as_deref() == Some(command.run_id.as_str())
            && application.state == relational_state
            && matches!(
                application.state.as_str(),
                "queued" | "running" | "needs_input"
            )
        {
            application.state = state.to_string();
            application.updated_at_ms = now_ms.max(application.updated_at_ms.saturating_add(1));
            let encoded = to_json(&application, "workflow terminal application")?;
            tx.execute(
                "UPDATE jobs_applications SET state = $1, application_json = $2,
                    updated_at_ms = $3 WHERE account_id = $4 AND id = $5 AND state = $6",
                &[
                    &state,
                    &encoded,
                    &application.updated_at_ms,
                    &command.account_id,
                    &command.application_id,
                    &relational_state,
                ],
            )?;
        }
    }
    let session_id = format!("cloud-{}", command.application_id);
    if let Some(row) = tx.query_opt(
        "SELECT session_json FROM jobs_browser_sessions
          WHERE id = $1 AND account_id = $2 AND runner = 'cloud' FOR UPDATE",
        &[&session_id, &command.account_id],
    )? {
        let mut session: BrowserSession =
            parse_json(row.get(0), "workflow terminal browser session")?;
        if session.application_id.as_deref() == Some(command.application_id.as_str())
            && matches!(
                session.status.as_str(),
                "queued" | "running" | "needs_input"
            )
        {
            session.status = state.to_string();
            session.updated_at_ms = now_ms.max(session.updated_at_ms.saturating_add(1));
            let encoded = to_json(&session, "workflow terminal browser session")?;
            tx.execute(
                "UPDATE jobs_browser_sessions SET status = $1, session_json = $2,
                    updated_at_ms = $3 WHERE id = $4 AND account_id = $5",
                &[
                    &state,
                    &encoded,
                    &session.updated_at_ms,
                    &session_id,
                    &command.account_id,
                ],
            )?;
        }
    }
    tx.execute(
        "UPDATE jobs_attempt_reservations SET status = $1, updated_at_ms = $2
          WHERE account_id = $3 AND application_id = $4
            AND runner = 'cloud' AND status IN ('reserved', 'running')",
        &[
            &state,
            &now_ms,
            &command.account_id,
            &command.application_id,
        ],
    )?;
    Ok(())
}

fn mark_workflow_execution_terminal_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE jobs_workflow_executions
            SET lifecycle_state = 'terminal', terminal_at_ms = COALESCE(terminal_at_ms, ?1),
                updated_at_ms = MAX(?1, updated_at_ms + 1)
          WHERE account_id = ?2 AND workflow_id = ?3
            AND lifecycle_state IN ('accepted', 'running', 'terminal')",
        params![now_ms, command.account_id, command.workflow_id],
    )?;
    Ok(())
}

fn mark_workflow_execution_terminal_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE jobs_workflow_executions
            SET lifecycle_state = 'terminal', terminal_at_ms = COALESCE(terminal_at_ms, $1),
                updated_at_ms = GREATEST($1, updated_at_ms + 1)
          WHERE account_id = $2 AND workflow_id = $3
            AND lifecycle_state IN ('accepted', 'running', 'terminal')",
        &[&now_ms, &command.account_id, &command.workflow_id],
    )?;
    Ok(())
}

fn terminalize_local_workflow_authority_after_absence_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    now_ms: i64,
) -> Result<()> {
    terminalize_workflow_rows_sqlite_tx(tx, command, "failed", now_ms)?;
    tx.execute(
        "UPDATE jobs_execution_leases
            SET phase = 'failed', finished_at_ms = COALESCE(finished_at_ms, ?1),
                updated_at_ms = MAX(?1, updated_at_ms + 1)
          WHERE account_id = ?2 AND application_id = ?3 AND run_id = ?4
            AND phase = 'prepared'",
        params![
            now_ms,
            command.account_id,
            command.application_id,
            command.run_id
        ],
    )?;
    Ok(())
}

fn terminalize_local_workflow_authority_after_absence_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    now_ms: i64,
) -> Result<()> {
    terminalize_workflow_rows_postgres_tx(tx, command, "failed", now_ms)?;
    tx.execute(
        "UPDATE jobs_execution_leases
            SET phase = 'failed', finished_at_ms = COALESCE(finished_at_ms, $1),
                updated_at_ms = GREATEST($1, updated_at_ms + 1)
          WHERE account_id = $2 AND application_id = $3 AND run_id = $4
            AND phase = 'prepared'",
        &[
            &now_ms,
            &command.account_id,
            &command.application_id,
            &command.run_id,
        ],
    )?;
    Ok(())
}

fn cancel_never_delivered_workflow_commands_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<i64> {
    Ok(tx.execute(
        "UPDATE jobs_workflow_commands
            SET state = 'cancelled', lease_owner = NULL, lease_token_sha256 = NULL,
                lease_expires_at_ms = NULL, next_attempt_at_ms = NULL,
                last_outcome_code = 'operator_cancelled',
                updated_at_ms = MAX(?1, updated_at_ms + 1)
          WHERE account_id = ?2 AND first_request_started_at_ms IS NULL
            AND state IN ('pending', 'claimed')",
        params![now_ms, account_id],
    )? as i64)
}

fn cancel_never_delivered_workflow_commands_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<i64> {
    Ok(tx.execute(
        "UPDATE jobs_workflow_commands
            SET state = 'cancelled', lease_owner = NULL, lease_token_sha256 = NULL,
                lease_expires_at_ms = NULL, next_attempt_at_ms = NULL,
                last_outcome_code = 'operator_cancelled',
                updated_at_ms = GREATEST($1, updated_at_ms + 1)
          WHERE account_id = $2 AND first_request_started_at_ms IS NULL
            AND state IN ('pending', 'claimed')",
        &[&now_ms, &account_id],
    )? as i64)
}

#[derive(Debug, Clone)]
struct FrozenWorkflowCleanupTarget {
    command_id: String,
    workflow_id: String,
    request_id: String,
    payload_hmac_sha256: String,
    first_execution_run_id: Option<String>,
    command_state: String,
    managed_cloud_binding_sha256: Option<String>,
    managed_cloud_release_memo_base64url: Option<String>,
    managed_cloud_release_memo_sha256: Option<String>,
}

struct StoredWorkflowCleanupLeaseRow {
    start_request_id: String,
    start_payload_hmac_sha256: String,
    first_execution_run_id: Option<String>,
    target_state: String,
    fence: i64,
    cleanup_request_id: Option<String>,
    lease_owner: Option<String>,
    lease_token_sha256: Option<String>,
    lease_expires_at_ms: Option<i64>,
}

fn sqlite_workflow_cleanup_lease_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredWorkflowCleanupLeaseRow> {
    Ok(StoredWorkflowCleanupLeaseRow {
        start_request_id: row.get(0)?,
        start_payload_hmac_sha256: row.get(1)?,
        first_execution_run_id: row.get(2)?,
        target_state: row.get(3)?,
        fence: row.get(4)?,
        cleanup_request_id: row.get(5)?,
        lease_owner: row.get(6)?,
        lease_token_sha256: row.get(7)?,
        lease_expires_at_ms: row.get(8)?,
    })
}

fn workflow_cleanup_targets_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    generation: i64,
    legacy_reconciled: bool,
    legacy_unresolved_count: i64,
) -> Result<(String, Vec<FrozenWorkflowCleanupTarget>)> {
    let mut statement = tx.prepare(
        "SELECT command.id, command.workflow_id, command.request_id,
                command.payload_hmac_sha256, execution.first_execution_run_id,
                command.state, binding.binding_sha256,
                binding.release_memo_base64url, binding.release_memo_sha256
           FROM jobs_workflow_commands command
           LEFT JOIN jobs_workflow_executions execution
             ON execution.account_id = command.account_id
            AND execution.workflow_id = command.workflow_id
            AND execution.start_command_id = command.id
           LEFT JOIN jobs_managed_cloud_workflow_bindings binding
             ON binding.command_id = command.id
          WHERE command.account_id = ?1 AND command.command_kind = 'start'
            AND command.first_request_started_at_ms IS NOT NULL
          ORDER BY command.workflow_id",
    )?;
    let targets = statement
        .query_map(params![account_id], |row| {
            Ok(FrozenWorkflowCleanupTarget {
                command_id: row.get(0)?,
                workflow_id: row.get(1)?,
                request_id: row.get(2)?,
                payload_hmac_sha256: row.get(3)?,
                first_execution_run_id: row.get(4)?,
                command_state: row.get(5)?,
                managed_cloud_binding_sha256: row.get(6)?,
                managed_cloud_release_memo_base64url: row.get(7)?,
                managed_cloud_release_memo_sha256: row.get(8)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let semantics = targets
        .iter()
        .map(frozen_workflow_cleanup_target_semantics)
        .collect::<Result<Vec<_>>>()?;
    let digest = workflow_command_hmac(
        "cleanup-target-set",
        &json!({
            "accountId": account_id,
            "generation": generation,
            "legacyReconciled": legacy_reconciled,
            "legacyUnresolvedCount": legacy_unresolved_count,
            "targets": semantics,
        }),
        WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
    )?;
    Ok((digest, targets))
}

fn workflow_cleanup_targets_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    generation: i64,
    legacy_reconciled: bool,
    legacy_unresolved_count: i64,
) -> Result<(String, Vec<FrozenWorkflowCleanupTarget>)> {
    let targets = tx
        .query(
            "SELECT command.id, command.workflow_id, command.request_id,
                    command.payload_hmac_sha256, execution.first_execution_run_id,
                    command.state, binding.binding_sha256,
                    binding.release_memo_base64url, binding.release_memo_sha256
               FROM jobs_workflow_commands command
               LEFT JOIN jobs_workflow_executions execution
                 ON execution.account_id = command.account_id
                AND execution.workflow_id = command.workflow_id
                AND execution.start_command_id = command.id
               LEFT JOIN jobs_managed_cloud_workflow_bindings binding
                 ON binding.command_id = command.id
              WHERE command.account_id = $1 AND command.command_kind = 'start'
                AND command.first_request_started_at_ms IS NOT NULL
              ORDER BY command.workflow_id FOR SHARE OF command",
            &[&account_id],
        )?
        .into_iter()
        .map(|row| FrozenWorkflowCleanupTarget {
            command_id: row.get(0),
            workflow_id: row.get(1),
            request_id: row.get(2),
            payload_hmac_sha256: row.get(3),
            first_execution_run_id: row.get(4),
            command_state: row.get(5),
            managed_cloud_binding_sha256: row.get(6),
            managed_cloud_release_memo_base64url: row.get(7),
            managed_cloud_release_memo_sha256: row.get(8),
        })
        .collect::<Vec<_>>();
    let semantics = targets
        .iter()
        .map(frozen_workflow_cleanup_target_semantics)
        .collect::<Result<Vec<_>>>()?;
    let digest = workflow_command_hmac(
        "cleanup-target-set",
        &json!({
            "accountId": account_id,
            "generation": generation,
            "legacyReconciled": legacy_reconciled,
            "legacyUnresolvedCount": legacy_unresolved_count,
            "targets": semantics,
        }),
        WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
    )?;
    Ok((digest, targets))
}

fn frozen_workflow_cleanup_target_semantics(target: &FrozenWorkflowCleanupTarget) -> Result<Value> {
    let mut semantics = json!({
        "workflowId": target.workflow_id,
        "startCommandId": target.command_id,
        "startRequestId": target.request_id,
        "startPayloadHmacSha256": target.payload_hmac_sha256,
        "firstExecutionRunIdAtFreeze": target.first_execution_run_id,
        "commandStateAtFreeze": target.command_state,
    });
    match (
        target.managed_cloud_binding_sha256.as_deref(),
        target.managed_cloud_release_memo_base64url.as_deref(),
        target.managed_cloud_release_memo_sha256.as_deref(),
    ) {
        (None, None, None) => {}
        (Some(binding_sha256), Some(_), Some(memo_sha256))
            if workflow_command_digest(binding_sha256) && workflow_command_digest(memo_sha256) =>
        {
            let object = semantics
                .as_object_mut()
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            object.insert(
                "managedCloudBindingSha256".to_string(),
                Value::String(binding_sha256.to_string()),
            );
            object.insert(
                "managedCloudReleaseMemoSha256".to_string(),
                Value::String(memo_sha256.to_string()),
            );
        }
        _ => return Err(JobsWorkflowCommandError::InvalidState.into()),
    }
    Ok(semantics)
}

fn workflow_cleanup_status_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    generation: i64,
    replayed: bool,
) -> Result<JobsWorkflowCleanupStatus> {
    let authority: (String, i64, bool, i64, String) = tx.query_row(
        "SELECT target_set_hmac_sha256, target_count, legacy_reconciled,
                legacy_unresolved_count, state
           FROM jobs_workflow_cleanup_generations
          WHERE account_id = ?1 AND generation = ?2",
        params![account_id, generation],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;
    let cancelled: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_commands
          WHERE account_id = ?1 AND first_request_started_at_ms IS NULL
            AND state IN ('cancelled', 'rejected')",
        params![account_id],
        |row| row.get(0),
    )?;
    let complete: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_targets
          WHERE account_id = ?1 AND generation = ?2
            AND target_set_hmac_sha256 = ?3 AND target_state = 'absence_proved'",
        params![account_id, generation, authority.0],
        |row| row.get(0),
    )?;
    let pending = authority.1.saturating_sub(complete);
    Ok(JobsWorkflowCleanupStatus {
        account_id: account_id.to_string(),
        generation,
        target_set_hmac_sha256: authority.0,
        target_count: authority.1,
        never_delivered_cancelled: cancelled,
        cleanup_complete: complete,
        cleanup_pending: pending,
        legacy_reconciled: authority.2,
        legacy_unresolved_count: authority.3,
        complete: authority.4 == "complete" && pending == 0 && authority.2 && authority.3 == 0,
        replayed,
    })
}

fn workflow_cleanup_status_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    generation: i64,
    replayed: bool,
) -> Result<JobsWorkflowCleanupStatus> {
    let authority = tx.query_one(
        "SELECT target_set_hmac_sha256, target_count, legacy_reconciled,
                legacy_unresolved_count, state
           FROM jobs_workflow_cleanup_generations
          WHERE account_id = $1 AND generation = $2 FOR SHARE",
        &[&account_id, &generation],
    )?;
    let digest: String = authority.get(0);
    let target_count: i64 = authority.get(1);
    let cancelled: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_workflow_commands
              WHERE account_id = $1 AND first_request_started_at_ms IS NULL
                AND state IN ('cancelled', 'rejected')",
            &[&account_id],
        )?
        .get(0);
    let complete_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets
              WHERE account_id = $1 AND generation = $2
                AND target_set_hmac_sha256 = $3 AND target_state = 'absence_proved'",
            &[&account_id, &generation, &digest],
        )?
        .get(0);
    let pending = target_count.saturating_sub(complete_count);
    let legacy_reconciled: bool = authority.get(2);
    let legacy_unresolved_count: i64 = authority.get(3);
    let state: String = authority.get(4);
    Ok(JobsWorkflowCleanupStatus {
        account_id: account_id.to_string(),
        generation,
        target_set_hmac_sha256: digest,
        target_count,
        never_delivered_cancelled: cancelled,
        cleanup_complete: complete_count,
        cleanup_pending: pending,
        legacy_reconciled,
        legacy_unresolved_count,
        complete: state == "complete"
            && pending == 0
            && legacy_reconciled
            && legacy_unresolved_count == 0,
        replayed,
    })
}

pub(crate) fn admit_jobs_workflow_command_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &NewJobsWorkflowCommand,
    managed_cloud_authority_required: bool,
) -> Result<JobsWorkflowCommandAdmission> {
    validate_new_workflow_command(input)?;
    crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(tx, &input.account_id)?;
    require_no_workflow_cleanup_sqlite_tx(tx, &input.account_id)?;
    let idempotency_key_hmac_sha256 = workflow_command_idempotency_hmac(input)?;
    let request_hmac_sha256 = workflow_command_request_hmac(input)?;
    let existing = tx
        .query_row(
            &format!(
                "SELECT {WORKFLOW_COMMAND_SELECT}
                   FROM jobs_workflow_commands
                  WHERE account_id = ?1 AND command_kind = ?2
                    AND idempotency_key_hmac_sha256 = ?3"
            ),
            params![
                input.account_id,
                input.command_kind.as_str(),
                idempotency_key_hmac_sha256,
            ],
            sqlite_workflow_command_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        return workflow_command_replay(existing, input, &request_hmac_sha256);
    }

    let hold_context = operational_hold_context_for_application_sqlite_tx(
        tx,
        &input.account_id,
        &input.application_id,
        Some("cloud"),
        None,
        None,
    )
    .map_err(anyhow::Error::new)?;
    require_operational_capability_sqlite_tx(
        tx,
        OperationalCapability::ApplicationQueue,
        &hold_context,
    )
    .map_err(anyhow::Error::new)?;

    let command_id = new_jobs_workflow_command_id();
    let request_id = workflow_command_request_id_for_idempotency(input)?;
    let payload_hmac_sha256 =
        workflow_command_payload_hmac(input, &request_id, &request_hmac_sha256)?;
    let envelope = workflow_command_envelope(
        input,
        request_id.clone(),
        request_hmac_sha256.clone(),
        payload_hmac_sha256.clone(),
    );
    let command_json = to_json(&envelope, "Jobs workflow command")?;
    tx.execute(
        "INSERT INTO jobs_workflow_commands (
            id, account_id, application_id, run_id, workflow_id, intervention_id,
            command_kind, protocol_version, idempotency_key_hmac_sha256, request_id,
            request_hmac_sha256, payload_hmac_sha256, state, command_json,
            managed_cloud_authority_required, attempt_count, fence,
            next_attempt_at_ms, created_at_ms, updated_at_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            'pending', ?13, ?14, 0, 0, ?15, ?15, ?15
         )",
        params![
            command_id,
            input.account_id,
            input.application_id,
            input.run_id,
            input.workflow_id,
            input.intervention_id,
            input.command_kind.as_str(),
            WORKFLOW_COMMAND_PROTOCOL_VERSION,
            idempotency_key_hmac_sha256,
            request_id,
            request_hmac_sha256,
            payload_hmac_sha256,
            command_json,
            if managed_cloud_authority_required {
                1_i64
            } else {
                0_i64
            },
            input.now_ms,
        ],
    )?;
    let stored = tx.query_row(
        &format!(
            "SELECT {WORKFLOW_COMMAND_SELECT}
               FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
        ),
        params![input.account_id, command_id],
        sqlite_workflow_command_row,
    )?;
    Ok(JobsWorkflowCommandAdmission {
        command: workflow_command_from_stored(stored)?,
        replayed: false,
    })
}

pub(crate) fn admit_jobs_workflow_command_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    input: &NewJobsWorkflowCommand,
    managed_cloud_authority_required: bool,
) -> Result<JobsWorkflowCommandAdmission> {
    validate_new_workflow_command(input)?;
    lock_operational_hold_shared_postgres_tx(tx).map_err(anyhow::Error::new)?;
    crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
        tx,
        &input.account_id,
    )?;
    require_no_workflow_cleanup_postgres_tx(tx, &input.account_id)?;
    let idempotency_key_hmac_sha256 = workflow_command_idempotency_hmac(input)?;
    let request_hmac_sha256 = workflow_command_request_hmac(input)?;
    let existing = tx.query_opt(
        &format!(
            "SELECT {WORKFLOW_COMMAND_SELECT}
               FROM jobs_workflow_commands
              WHERE account_id = $1 AND command_kind = $2
                AND idempotency_key_hmac_sha256 = $3
              FOR SHARE"
        ),
        &[
            &input.account_id,
            &input.command_kind.as_str(),
            &idempotency_key_hmac_sha256,
        ],
    )?;
    if let Some(existing) = existing {
        return workflow_command_replay(
            postgres_workflow_command_row(&existing),
            input,
            &request_hmac_sha256,
        );
    }

    let hold_context = operational_hold_context_for_application_postgres_tx(
        tx,
        &input.account_id,
        &input.application_id,
        Some("cloud"),
        None,
        None,
    )
    .map_err(anyhow::Error::new)?;
    require_operational_capability_postgres_tx(
        tx,
        OperationalCapability::ApplicationQueue,
        &hold_context,
    )
    .map_err(anyhow::Error::new)?;

    let command_id = new_jobs_workflow_command_id();
    let request_id = workflow_command_request_id_for_idempotency(input)?;
    let payload_hmac_sha256 =
        workflow_command_payload_hmac(input, &request_id, &request_hmac_sha256)?;
    let envelope = workflow_command_envelope(
        input,
        request_id.clone(),
        request_hmac_sha256.clone(),
        payload_hmac_sha256.clone(),
    );
    let command_json = to_json(&envelope, "Jobs workflow command")?;
    tx.execute(
        "INSERT INTO jobs_workflow_commands (
            id, account_id, application_id, run_id, workflow_id, intervention_id,
            command_kind, protocol_version, idempotency_key_hmac_sha256, request_id,
            request_hmac_sha256, payload_hmac_sha256, state, command_json,
            managed_cloud_authority_required, attempt_count, fence,
            next_attempt_at_ms, created_at_ms, updated_at_ms
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
            'pending', $13, $14, 0, 0, $15, $15, $15
         )",
        &[
            &command_id,
            &input.account_id,
            &input.application_id,
            &input.run_id,
            &input.workflow_id,
            &input.intervention_id,
            &input.command_kind.as_str(),
            &WORKFLOW_COMMAND_PROTOCOL_VERSION,
            &idempotency_key_hmac_sha256,
            &request_id,
            &request_hmac_sha256,
            &payload_hmac_sha256,
            &command_json,
            &managed_cloud_authority_required,
            &input.now_ms,
        ],
    )?;
    let stored = tx.query_one(
        &format!(
            "SELECT {WORKFLOW_COMMAND_SELECT}
               FROM jobs_workflow_commands WHERE account_id = $1 AND id = $2"
        ),
        &[&input.account_id, &command_id],
    )?;
    Ok(JobsWorkflowCommandAdmission {
        command: workflow_command_from_stored(postgres_workflow_command_row(&stored))?,
        replayed: false,
    })
}

pub fn admit_jobs_workflow_command(
    pool: &DbPool,
    input: &NewJobsWorkflowCommand,
) -> Result<JobsWorkflowCommandAdmission> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let admission = admit_jobs_workflow_command_sqlite_tx(&transaction, input, false)?;
            transaction.commit()?;
            Ok(admission)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let admission =
                admit_jobs_workflow_command_postgres_tx(&mut transaction, input, false)?;
            transaction.commit()?;
            Ok(admission)
        }
    })
}

pub fn get_jobs_workflow_command(
    pool: &DbPool,
    account_id: &str,
    command_id: &str,
) -> Result<Option<JobsWorkflowCommand>> {
    if !workflow_command_identifier(account_id, 128)
        || !workflow_command_identifier(command_id, 128)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
                ),
                params![account_id, command_id],
                sqlite_workflow_command_row,
            )
            .optional()?
            .map(workflow_command_from_stored)
            .transpose(),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = $1 AND id = $2"
                ),
                &[&account_id, &command_id],
            )?
            .as_ref()
            .map(postgres_workflow_command_row)
            .map(workflow_command_from_stored)
            .transpose(),
    })
}

pub fn get_materializable_jobs_workflow_command_by_request_id(
    pool: &DbPool,
    request_id: &str,
) -> Result<Option<JobsWorkflowCommand>> {
    if !workflow_command_identifier(request_id, 128) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands command
                      WHERE command.request_id = ?1
                        AND command.state IN ('delivering', 'delivery_unknown', 'accepted')
                        AND command.first_request_started_at_ms IS NOT NULL
                        AND EXISTS (
                          SELECT 1 FROM jobs_workflow_command_attempt_events event
                           WHERE event.command_id = command.id
                             AND event.event_kind = 'request_started'
                        )
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_workflow_cleanup_generations cleanup
                           WHERE cleanup.account_id = command.account_id
                        )"
                    ),
                    params![request_id],
                    sqlite_workflow_command_row,
                )
                .optional()?;
            let Some(row) = row else {
                transaction.commit()?;
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &transaction,
                &row.account_id,
            )?;
            let command = workflow_command_from_stored(row)?;
            transaction.commit()?;
            Ok(Some(command))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let identity = transaction.query_opt(
                "SELECT command.account_id, command.id
                       FROM jobs_workflow_commands command
                      WHERE command.request_id = $1
                        AND command.state IN ('delivering', 'delivery_unknown', 'accepted')
                        AND command.first_request_started_at_ms IS NOT NULL
                        AND EXISTS (
                          SELECT 1 FROM jobs_workflow_command_attempt_events event
                           WHERE event.command_id = command.id
                             AND event.event_kind = 'request_started'
                        )
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_workflow_cleanup_generations cleanup
                           WHERE cleanup.account_id = command.account_id
                        )",
                &[&request_id],
            )?;
            let Some(identity) = identity else {
                transaction.commit()?;
                return Ok(None);
            };
            let account_id: String = identity.get(0);
            let command_id: String = identity.get(1);
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut transaction,
                &account_id,
            )?;
            let row = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands command
                      WHERE command.account_id = $1 AND command.id = $2
                        AND command.request_id = $3
                        AND command.state IN ('delivering', 'delivery_unknown', 'accepted')
                        AND command.first_request_started_at_ms IS NOT NULL
                        AND EXISTS (
                          SELECT 1 FROM jobs_workflow_command_attempt_events event
                           WHERE event.command_id = command.id
                             AND event.event_kind = 'request_started'
                        )
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_workflow_cleanup_generations cleanup
                           WHERE cleanup.account_id = command.account_id
                        )
                      FOR SHARE"
                ),
                &[&account_id, &command_id, &request_id],
            )?;
            let Some(row) = row else {
                transaction.commit()?;
                return Ok(None);
            };
            let row = postgres_workflow_command_row(&row);
            let command = workflow_command_from_stored(row)?;
            transaction.commit()?;
            Ok(Some(command))
        }
    })
}

pub fn prepare_jobs_workflow_intervention(
    pool: &DbPool,
    input: &PrepareJobsWorkflowIntervention,
) -> Result<PreparedJobsWorkflowIntervention> {
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let command = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE request_id = ?1"
                    ),
                    params![input.request_id],
                    sqlite_workflow_command_row,
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            validate_external_workflow_authority(
                &command,
                &input.request_id,
                &input.payload_hmac_sha256,
                &input.workflow_id,
                input.command_kind,
                input.intervention_id.as_deref(),
            )?;
            require_no_workflow_cleanup_sqlite_tx(&transaction, &command.account_id)?;
            let intervention_id = deterministic_workflow_intervention_id(&command)?;
            let mut intervention =
                intervention_from_workflow_receipt(&input.receipt, &intervention_id)?;
            intervention.application_id = Some(command.application_id.clone());
            intervention.created_at_ms = input.now_ms;
            let receipt_hmac = workflow_command_hmac(
                "intervention-receipt",
                &input.receipt,
                WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
            )?;
            let encrypted = to_json(&intervention, "prepared Jobs workflow intervention")?;
            let changed = transaction.execute(
                "INSERT INTO jobs_workflow_intervention_preparations (
                    command_id, account_id, request_id, payload_hmac_sha256,
                    intervention_id, receipt_hmac_sha256, intervention_json,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(command_id) DO NOTHING",
                params![
                    command.id,
                    command.account_id,
                    command.request_id,
                    command.payload_hmac_sha256,
                    intervention_id,
                    receipt_hmac,
                    encrypted,
                    input.now_ms,
                ],
            )?;
            let stored: (String, String, String, String) = transaction.query_row(
                "SELECT request_id, payload_hmac_sha256, intervention_id, receipt_hmac_sha256
                   FROM jobs_workflow_intervention_preparations WHERE command_id = ?1",
                params![command.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
            if stored.0 != input.request_id
                || !workflow_command_hmac_matches(&stored.1, &input.payload_hmac_sha256)
                || stored.2 != intervention_id
                || !workflow_command_hmac_matches(&stored.3, &receipt_hmac)
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            transaction.commit()?;
            Ok(PreparedJobsWorkflowIntervention {
                request_id: stored.0,
                payload_hmac_sha256: stored.1,
                intervention_id: stored.2,
                replayed: changed == 0,
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let row = transaction
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE request_id = $1 FOR SHARE"
                    ),
                    &[&input.request_id],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let command = postgres_workflow_command_row(&row);
            validate_external_workflow_authority(
                &command,
                &input.request_id,
                &input.payload_hmac_sha256,
                &input.workflow_id,
                input.command_kind,
                input.intervention_id.as_deref(),
            )?;
            require_no_workflow_cleanup_postgres_tx(&mut transaction, &command.account_id)?;
            let intervention_id = deterministic_workflow_intervention_id(&command)?;
            let mut intervention =
                intervention_from_workflow_receipt(&input.receipt, &intervention_id)?;
            intervention.application_id = Some(command.application_id.clone());
            intervention.created_at_ms = input.now_ms;
            let receipt_hmac = workflow_command_hmac(
                "intervention-receipt",
                &input.receipt,
                WORKFLOW_COMMAND_PAYLOAD_MAX_BYTES,
            )?;
            let encrypted = to_json(&intervention, "prepared Jobs workflow intervention")?;
            let changed = transaction.execute(
                "INSERT INTO jobs_workflow_intervention_preparations (
                    command_id, account_id, request_id, payload_hmac_sha256,
                    intervention_id, receipt_hmac_sha256, intervention_json,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
                 ON CONFLICT(command_id) DO NOTHING",
                &[
                    &command.id,
                    &command.account_id,
                    &command.request_id,
                    &command.payload_hmac_sha256,
                    &intervention_id,
                    &receipt_hmac,
                    &encrypted,
                    &input.now_ms,
                ],
            )?;
            let stored = transaction.query_one(
                "SELECT request_id, payload_hmac_sha256, intervention_id, receipt_hmac_sha256
                   FROM jobs_workflow_intervention_preparations
                  WHERE command_id = $1 FOR SHARE",
                &[&command.id],
            )?;
            let stored_request_id: String = stored.get(0);
            let stored_payload_hmac: String = stored.get(1);
            let stored_intervention_id: String = stored.get(2);
            let stored_receipt_hmac: String = stored.get(3);
            if stored_request_id != input.request_id
                || !workflow_command_hmac_matches(&stored_payload_hmac, &input.payload_hmac_sha256)
                || stored_intervention_id != intervention_id
                || !workflow_command_hmac_matches(&stored_receipt_hmac, &receipt_hmac)
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            transaction.commit()?;
            Ok(PreparedJobsWorkflowIntervention {
                request_id: stored_request_id,
                payload_hmac_sha256: stored_payload_hmac,
                intervention_id: stored_intervention_id,
                replayed: changed == 0,
            })
        }
    })
}

fn publish_intervention_rows_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    intervention: &Intervention,
    encrypted: &str,
    now_ms: i64,
) -> Result<()> {
    let application_row: (String, String, String) = tx.query_row(
        SQLITE_WORKFLOW_APPLICATION_AUTHORITY_SQL,
        params![command.account_id, command.application_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let mut application = parse_application_json(
        application_row.0,
        &command.application_id,
        &application_row.2,
        "workflow intervention application",
    )?;
    if application.run_id.as_deref() != Some(command.run_id.as_str())
        || application.state != application_row.1
        || !matches!(
            application.state.as_str(),
            "queued" | "running" | "needs_input"
        )
    {
        anyhow::bail!("workflow application cannot publish this intervention")
    }
    let inserted = tx.execute(
        "INSERT INTO jobs_interventions (
            id, account_id, application_id, kind, status, intervention_json,
            created_at_ms, resolved_at_ms
         ) VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, NULL)
         ON CONFLICT(id) DO NOTHING",
        params![
            intervention.id,
            command.account_id,
            command.application_id,
            intervention.kind,
            encrypted,
            intervention.created_at_ms,
        ],
    )?;
    if inserted == 0 {
        let authority: Option<(String, String, String)> = tx
            .query_row(
                "SELECT account_id, application_id, intervention_json
                   FROM jobs_interventions WHERE id = ?1",
                params![intervention.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if authority.as_ref().is_none_or(|row| {
            row.0 != command.account_id || row.1 != command.application_id || row.2 != encrypted
        }) {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        return Ok(());
    }
    application.state = "needs_input".to_string();
    application.updated_at_ms = now_ms.max(application.updated_at_ms.saturating_add(1));
    let application_json = to_json(&application, "workflow intervention application")?;
    if tx.execute(
        "UPDATE jobs_applications SET state = 'needs_input', application_json = ?1,
            updated_at_ms = ?2 WHERE account_id = ?3 AND id = ?4 AND state = ?5",
        params![
            application_json,
            application.updated_at_ms,
            command.account_id,
            command.application_id,
            application_row.1,
        ],
    )? != 1
    {
        anyhow::bail!("workflow application changed during intervention publication")
    }
    let session_id = format!("cloud-{}", command.application_id);
    let session_row: (String, String) = tx.query_row(
        "SELECT session_json, status FROM jobs_browser_sessions
          WHERE id = ?1 AND account_id = ?2 AND runner = 'cloud'",
        params![session_id, command.account_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut session: BrowserSession =
        parse_json(session_row.0, "workflow intervention browser session")?;
    if session.application_id.as_deref() != Some(command.application_id.as_str())
        || !matches!(
            session.status.as_str(),
            "queued" | "running" | "needs_input"
        )
    {
        anyhow::bail!("workflow browser session cannot publish this intervention")
    }
    session.status = "needs_input".to_string();
    session.current_step = intervention.title.clone();
    session.takeover_url = intervention
        .metadata
        .pointer("/receipt/intervention/takeoverUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    session.updated_at_ms = now_ms.max(session.updated_at_ms.saturating_add(1));
    let session_json = to_json(&session, "workflow intervention browser session")?;
    if tx.execute(
        "UPDATE jobs_browser_sessions SET status = 'needs_input', session_json = ?1,
            updated_at_ms = ?2 WHERE id = ?3 AND account_id = ?4 AND status = ?5",
        params![
            session_json,
            session.updated_at_ms,
            session_id,
            command.account_id,
            session_row.1
        ],
    )? != 1
    {
        anyhow::bail!("workflow browser session changed during intervention publication")
    }
    Ok(())
}

fn publish_intervention_rows_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    intervention: &Intervention,
    encrypted: &str,
    now_ms: i64,
) -> Result<()> {
    let application_row = tx.query_one(
        POSTGRES_WORKFLOW_APPLICATION_AUTHORITY_SQL,
        &[&command.account_id, &command.application_id],
    )?;
    let application_state: String = application_row.get(1);
    let job_id: String = application_row.get(2);
    let mut application = parse_application_json(
        application_row.get(0),
        &command.application_id,
        &job_id,
        "workflow intervention application",
    )?;
    if application.run_id.as_deref() != Some(command.run_id.as_str())
        || application.state != application_state
        || !matches!(
            application.state.as_str(),
            "queued" | "running" | "needs_input"
        )
    {
        anyhow::bail!("workflow application cannot publish this intervention")
    }
    let inserted = tx.execute(
        "INSERT INTO jobs_interventions (
            id, account_id, application_id, kind, status, intervention_json,
            created_at_ms, resolved_at_ms
         ) VALUES ($1, $2, $3, $4, 'open', $5, $6, NULL)
         ON CONFLICT(id) DO NOTHING",
        &[
            &intervention.id,
            &command.account_id,
            &command.application_id,
            &intervention.kind,
            &encrypted,
            &intervention.created_at_ms,
        ],
    )?;
    if inserted == 0 {
        let authority = tx.query_opt(
            "SELECT account_id, application_id, intervention_json
               FROM jobs_interventions WHERE id = $1 FOR SHARE",
            &[&intervention.id],
        )?;
        if authority.as_ref().is_none_or(|row| {
            row.get::<_, String>(0) != command.account_id
                || row.get::<_, String>(1) != command.application_id
                || row.get::<_, String>(2) != encrypted
        }) {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
        return Ok(());
    }
    application.state = "needs_input".to_string();
    application.updated_at_ms = now_ms.max(application.updated_at_ms.saturating_add(1));
    let application_json = to_json(&application, "workflow intervention application")?;
    if tx.execute(
        "UPDATE jobs_applications SET state = 'needs_input', application_json = $1,
            updated_at_ms = $2 WHERE account_id = $3 AND id = $4 AND state = $5",
        &[
            &application_json,
            &application.updated_at_ms,
            &command.account_id,
            &command.application_id,
            &application_state,
        ],
    )? != 1
    {
        anyhow::bail!("workflow application changed during intervention publication")
    }
    let session_id = format!("cloud-{}", command.application_id);
    let session_row = tx.query_one(
        "SELECT session_json, status FROM jobs_browser_sessions
          WHERE id = $1 AND account_id = $2 AND runner = 'cloud' FOR UPDATE",
        &[&session_id, &command.account_id],
    )?;
    let session_state: String = session_row.get(1);
    let mut session: BrowserSession =
        parse_json(session_row.get(0), "workflow intervention browser session")?;
    if session.application_id.as_deref() != Some(command.application_id.as_str())
        || !matches!(
            session.status.as_str(),
            "queued" | "running" | "needs_input"
        )
    {
        anyhow::bail!("workflow browser session cannot publish this intervention")
    }
    session.status = "needs_input".to_string();
    session.current_step = intervention.title.clone();
    session.takeover_url = intervention
        .metadata
        .pointer("/receipt/intervention/takeoverUrl")
        .and_then(Value::as_str)
        .map(str::to_string);
    session.updated_at_ms = now_ms.max(session.updated_at_ms.saturating_add(1));
    let session_json = to_json(&session, "workflow intervention browser session")?;
    if tx.execute(
        "UPDATE jobs_browser_sessions SET status = 'needs_input', session_json = $1,
            updated_at_ms = $2 WHERE id = $3 AND account_id = $4 AND status = $5",
        &[
            &session_json,
            &session.updated_at_ms,
            &session_id,
            &command.account_id,
            &session_state,
        ],
    )? != 1
    {
        anyhow::bail!("workflow browser session changed during intervention publication")
    }
    Ok(())
}

pub fn publish_jobs_workflow_intervention(
    pool: &DbPool,
    input: &PublishJobsWorkflowIntervention,
) -> Result<PublishedJobsWorkflowIntervention> {
    if !workflow_command_opaque_identifier(&input.intervention_id, 128)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let command = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE request_id = ?1"
                    ),
                    params![input.request_id],
                    sqlite_workflow_command_row,
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            validate_external_workflow_authority(
                &command,
                &input.request_id,
                &input.payload_hmac_sha256,
                &input.workflow_id,
                input.command_kind,
                input.command_intervention_id.as_deref(),
            )?;
            require_no_workflow_cleanup_sqlite_tx(&transaction, &command.account_id)?;
            let prepared: (String, Option<i64>) = transaction
                .query_row(
                    "SELECT intervention_json, published_at_ms
                       FROM jobs_workflow_intervention_preparations
                      WHERE command_id = ?1 AND account_id = ?2 AND request_id = ?3
                        AND payload_hmac_sha256 = ?4 AND intervention_id = ?5",
                    params![
                        command.id,
                        command.account_id,
                        command.request_id,
                        command.payload_hmac_sha256,
                        input.intervention_id
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let intervention: Intervention =
                parse_json(prepared.0.clone(), "prepared Jobs workflow intervention")?;
            if prepared.1.is_none() {
                publish_intervention_rows_sqlite_tx(
                    &transaction,
                    &command,
                    &intervention,
                    &prepared.0,
                    input.now_ms,
                )?;
                if transaction.execute(
                    "UPDATE jobs_workflow_intervention_preparations
                        SET published_at_ms = ?1, updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE command_id = ?2 AND published_at_ms IS NULL",
                    params![input.now_ms, command.id],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            } else {
                let stored: Option<String> = transaction
                    .query_row(
                        "SELECT intervention_json FROM jobs_interventions
                          WHERE id = ?1 AND account_id = ?2 AND application_id = ?3",
                        params![
                            input.intervention_id,
                            command.account_id,
                            command.application_id
                        ],
                        |row| row.get(0),
                    )
                    .optional()?;
                let stored_intervention = stored
                    .map(|raw| parse_json::<Intervention>(raw, "published Jobs intervention"))
                    .transpose()?;
                if stored_intervention.as_ref().is_none_or(|actual| {
                    !workflow_intervention_replay_matches(&intervention, actual, &command)
                        .unwrap_or(false)
                }) {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            transaction.commit()?;
            Ok(PublishedJobsWorkflowIntervention {
                intervention,
                replayed: prepared.1.is_some(),
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let row = transaction
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE request_id = $1 FOR SHARE"
                    ),
                    &[&input.request_id],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let command = postgres_workflow_command_row(&row);
            validate_external_workflow_authority(
                &command,
                &input.request_id,
                &input.payload_hmac_sha256,
                &input.workflow_id,
                input.command_kind,
                input.command_intervention_id.as_deref(),
            )?;
            require_no_workflow_cleanup_postgres_tx(&mut transaction, &command.account_id)?;
            let prepared = transaction
                .query_opt(
                    "SELECT intervention_json, published_at_ms
                       FROM jobs_workflow_intervention_preparations
                      WHERE command_id = $1 AND account_id = $2 AND request_id = $3
                        AND payload_hmac_sha256 = $4 AND intervention_id = $5 FOR UPDATE",
                    &[
                        &command.id,
                        &command.account_id,
                        &command.request_id,
                        &command.payload_hmac_sha256,
                        &input.intervention_id,
                    ],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let encrypted: String = prepared.get(0);
            let published_at_ms: Option<i64> = prepared.get(1);
            let intervention: Intervention =
                parse_json(encrypted.clone(), "prepared Jobs workflow intervention")?;
            if published_at_ms.is_none() {
                publish_intervention_rows_postgres_tx(
                    &mut transaction,
                    &command,
                    &intervention,
                    &encrypted,
                    input.now_ms,
                )?;
                if transaction.execute(
                    "UPDATE jobs_workflow_intervention_preparations
                        SET published_at_ms = $1,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE command_id = $2 AND published_at_ms IS NULL",
                    &[&input.now_ms, &command.id],
                )? != 1
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            } else {
                let stored = transaction.query_opt(
                    "SELECT intervention_json FROM jobs_interventions
                      WHERE id = $1 AND account_id = $2 AND application_id = $3 FOR SHARE",
                    &[
                        &input.intervention_id,
                        &command.account_id,
                        &command.application_id,
                    ],
                )?;
                let stored_intervention = stored
                    .map(|row| {
                        parse_json::<Intervention>(row.get(0), "published Jobs intervention")
                    })
                    .transpose()?;
                if stored_intervention.as_ref().is_none_or(|actual| {
                    !workflow_intervention_replay_matches(&intervention, actual, &command)
                        .unwrap_or(false)
                }) {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            transaction.commit()?;
            Ok(PublishedJobsWorkflowIntervention {
                intervention,
                replayed: published_at_ms.is_some(),
            })
        }
    })
}

fn close_exact_workflow_intervention_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    intervention_id: &str,
    now_ms: i64,
) -> Result<()> {
    let row = tx
        .query_row(
            "SELECT intervention_json, status FROM jobs_interventions
              WHERE id = ?1 AND account_id = ?2 AND application_id = ?3",
            params![intervention_id, command.account_id, command.application_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or(JobsWorkflowCommandError::NotFound)?;
    if row.1 != "open" {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let mut intervention: Intervention = parse_json(row.0, "workflow terminal intervention")?;
    if intervention.id != intervention_id
        || intervention.application_id.as_deref() != Some(command.application_id.as_str())
        || intervention.status != "open"
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    intervention.status = "cancelled".to_string();
    intervention.resolved_at_ms = Some(now_ms);
    let encoded = to_json(&intervention, "workflow terminal intervention")?;
    if tx.execute(
        "UPDATE jobs_interventions SET status = 'cancelled', intervention_json = ?1,
            resolved_at_ms = ?2 WHERE id = ?3 AND account_id = ?4
              AND application_id = ?5 AND status = 'open'",
        params![
            encoded,
            now_ms,
            intervention_id,
            command.account_id,
            command.application_id
        ],
    )? != 1
    {
        anyhow::bail!("workflow intervention changed during finalization")
    }
    Ok(())
}

fn close_exact_workflow_intervention_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
    intervention_id: &str,
    now_ms: i64,
) -> Result<()> {
    let row = tx
        .query_opt(
            "SELECT intervention_json, status FROM jobs_interventions
              WHERE id = $1 AND account_id = $2 AND application_id = $3 FOR UPDATE",
            &[
                &intervention_id,
                &command.account_id,
                &command.application_id,
            ],
        )?
        .ok_or(JobsWorkflowCommandError::NotFound)?;
    if row.get::<_, String>(1) != "open" {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    let mut intervention: Intervention = parse_json(row.get(0), "workflow terminal intervention")?;
    if intervention.id != intervention_id
        || intervention.application_id.as_deref() != Some(command.application_id.as_str())
        || intervention.status != "open"
    {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    intervention.status = "cancelled".to_string();
    intervention.resolved_at_ms = Some(now_ms);
    let encoded = to_json(&intervention, "workflow terminal intervention")?;
    if tx.execute(
        "UPDATE jobs_interventions SET status = 'cancelled', intervention_json = $1,
            resolved_at_ms = $2 WHERE id = $3 AND account_id = $4
              AND application_id = $5 AND status = 'open'",
        &[
            &encoded,
            &now_ms,
            &intervention_id,
            &command.account_id,
            &command.application_id,
        ],
    )? != 1
    {
        anyhow::bail!("workflow intervention changed during finalization")
    }
    Ok(())
}

fn require_no_open_runner_intervention_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
) -> Result<()> {
    tx.query_row(
        SQLITE_WORKFLOW_APPLICATION_AUTHORITY_SQL,
        params![command.account_id, command.application_id],
        |_| Ok(()),
    )
    .optional()?
    .ok_or(JobsWorkflowCommandError::NotFound)?;
    let open_count: i64 = tx.query_row(
        SQLITE_OPEN_APPLICATION_INTERVENTIONS_SQL,
        params![command.account_id, command.application_id],
        |row| row.get(0),
    )?;
    if open_count != 0 {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    Ok(())
}

fn require_no_open_runner_intervention_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredWorkflowCommandRow,
) -> Result<()> {
    // Publication takes this same row lock before inserting an open intervention. Holding it
    // through the absence proof and terminal writes closes the Postgres publication race.
    tx.query_opt(
        POSTGRES_WORKFLOW_APPLICATION_AUTHORITY_SQL,
        &[&command.account_id, &command.application_id],
    )?
    .ok_or(JobsWorkflowCommandError::NotFound)?;
    let open_count: i64 = tx
        .query_one(
            POSTGRES_OPEN_APPLICATION_INTERVENTIONS_SQL,
            &[&command.account_id, &command.application_id],
        )?
        .get(0);
    if open_count != 0 {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    Ok(())
}

pub fn finalize_jobs_workflow_execution(
    pool: &DbPool,
    input: &FinalizeJobsWorkflowExecution,
) -> Result<JobsWorkflowExecutionFinalization> {
    if !input.outcome.valid()
        || input
            .open_intervention_id
            .as_deref()
            .is_some_and(|value| !workflow_command_opaque_identifier(value, 128))
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let outcome_kind = input.outcome.state();
    let reason_code = input.outcome.reason().as_str();
    let intervention_binding_valid = match input.outcome.reason() {
        JobsWorkflowTerminalReason::InterventionTimeout => input.open_intervention_id.is_some(),
        JobsWorkflowTerminalReason::RunnerFailed
        | JobsWorkflowTerminalReason::RunnerAmbiguous
        | JobsWorkflowTerminalReason::InterventionLimit => input.open_intervention_id.is_none(),
    };
    if !intervention_binding_valid {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let outcome_hmac = workflow_command_hmac(
        "execution-finalization",
        &json!({
            "requestId": input.request_id,
            "payloadHmacSha256": input.payload_hmac_sha256,
            "workflowId": input.workflow_id,
            "commandKind": input.command_kind,
            "interventionId": input.intervention_id,
            "outcomeKind": outcome_kind,
            "reasonCode": reason_code,
            "openInterventionId": input.open_intervention_id,
        }),
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let command = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE request_id = ?1"
                    ),
                    params![input.request_id],
                    sqlite_workflow_command_row,
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            validate_external_workflow_authority(
                &command,
                &input.request_id,
                &input.payload_hmac_sha256,
                &input.workflow_id,
                input.command_kind,
                input.intervention_id.as_deref(),
            )?;
            if let Some(stored) = transaction
                .query_row(
                    "SELECT outcome_hmac_sha256
                       FROM jobs_workflow_execution_finalizations
                      WHERE command_id = ?1",
                    params![command.id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            {
                if !workflow_command_hmac_matches(&stored, &outcome_hmac) {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                transaction.commit()?;
                return Ok(JobsWorkflowExecutionFinalization {
                    request_id: input.request_id.clone(),
                    outcome: input.outcome,
                    replayed: true,
                });
            }
            match input.outcome.reason() {
                JobsWorkflowTerminalReason::InterventionTimeout => {
                    let intervention_id = input
                        .open_intervention_id
                        .as_deref()
                        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
                    let published: i64 = transaction.query_row(
                        "SELECT COUNT(*) FROM jobs_workflow_intervention_preparations preparation
                          JOIN jobs_interventions intervention
                            ON intervention.id = preparation.intervention_id
                           AND intervention.account_id = preparation.account_id
                         WHERE preparation.command_id = ?1
                           AND preparation.intervention_id = ?2
                           AND preparation.published_at_ms IS NOT NULL
                           AND intervention.application_id = ?3
                           AND intervention.status = 'open'",
                        params![command.id, intervention_id, command.application_id],
                        |row| row.get(0),
                    )?;
                    if published != 1 {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                }
                JobsWorkflowTerminalReason::InterventionLimit => {
                    let unpublished: i64 = transaction.query_row(
                        "SELECT COUNT(*) FROM jobs_workflow_intervention_preparations
                          WHERE command_id = ?1 AND published_at_ms IS NULL",
                        params![command.id],
                        |row| row.get(0),
                    )?;
                    if unpublished != 1 {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                }
                JobsWorkflowTerminalReason::RunnerFailed
                | JobsWorkflowTerminalReason::RunnerAmbiguous => {
                    require_no_open_runner_intervention_sqlite_tx(&transaction, &command)?;
                }
            }
            let changed = transaction.execute(
                "INSERT INTO jobs_workflow_execution_finalizations (
                    command_id, account_id, request_id, payload_hmac_sha256,
                    outcome_kind, reason_code, open_intervention_id,
                    outcome_hmac_sha256, finalized_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(command_id) DO NOTHING",
                params![
                    command.id,
                    command.account_id,
                    command.request_id,
                    command.payload_hmac_sha256,
                    outcome_kind,
                    reason_code,
                    input.open_intervention_id,
                    outcome_hmac,
                    input.now_ms
                ],
            )?;
            let stored: String = transaction.query_row(
                "SELECT outcome_hmac_sha256 FROM jobs_workflow_execution_finalizations
                  WHERE command_id = ?1",
                params![command.id],
                |row| row.get(0),
            )?;
            if !workflow_command_hmac_matches(&stored, &outcome_hmac) {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if changed == 1 {
                if let Some(intervention_id) = input.open_intervention_id.as_deref() {
                    close_exact_workflow_intervention_sqlite_tx(
                        &transaction,
                        &command,
                        intervention_id,
                        input.now_ms,
                    )?;
                }
                terminalize_workflow_rows_sqlite_tx(
                    &transaction,
                    &command,
                    outcome_kind,
                    input.now_ms,
                )?;
                mark_workflow_execution_terminal_sqlite_tx(&transaction, &command, input.now_ms)?;
                let discarded_hidden_prompt = input.outcome
                    == JobsWorkflowTerminalOutcome::Failed(
                        JobsWorkflowTerminalReason::InterventionLimit,
                    )
                    && input.open_intervention_id.is_none()
                    && transaction.execute(
                        "DELETE FROM jobs_workflow_intervention_preparations
                          WHERE command_id = ?1 AND published_at_ms IS NULL",
                        params![command.id],
                    )? == 1;
                if input.outcome
                    == JobsWorkflowTerminalOutcome::Failed(
                        JobsWorkflowTerminalReason::InterventionLimit,
                    )
                    && !discarded_hidden_prompt
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            transaction.commit()?;
            Ok(JobsWorkflowExecutionFinalization {
                request_id: input.request_id.clone(),
                outcome: input.outcome,
                replayed: changed == 0,
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let row = transaction
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands
                          WHERE request_id = $1 FOR SHARE"
                    ),
                    &[&input.request_id],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let command = postgres_workflow_command_row(&row);
            validate_external_workflow_authority(
                &command,
                &input.request_id,
                &input.payload_hmac_sha256,
                &input.workflow_id,
                input.command_kind,
                input.intervention_id.as_deref(),
            )?;
            if let Some(row) = transaction.query_opt(
                "SELECT outcome_hmac_sha256
                   FROM jobs_workflow_execution_finalizations
                  WHERE command_id = $1 FOR SHARE",
                &[&command.id],
            )? {
                let stored: String = row.get(0);
                if !workflow_command_hmac_matches(&stored, &outcome_hmac) {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                transaction.commit()?;
                return Ok(JobsWorkflowExecutionFinalization {
                    request_id: input.request_id.clone(),
                    outcome: input.outcome,
                    replayed: true,
                });
            }
            match input.outcome.reason() {
                JobsWorkflowTerminalReason::InterventionTimeout => {
                    let intervention_id = input
                        .open_intervention_id
                        .as_deref()
                        .ok_or(JobsWorkflowCommandError::InvalidRequest)?;
                    let published: i64 = transaction
                        .query_one(
                            "SELECT COUNT(*)::bigint
                               FROM jobs_workflow_intervention_preparations preparation
                               JOIN jobs_interventions intervention
                                 ON intervention.id = preparation.intervention_id
                                AND intervention.account_id = preparation.account_id
                              WHERE preparation.command_id = $1
                                AND preparation.intervention_id = $2
                                AND preparation.published_at_ms IS NOT NULL
                                AND intervention.application_id = $3
                                AND intervention.status = 'open'",
                            &[&command.id, &intervention_id, &command.application_id],
                        )?
                        .get(0);
                    if published != 1 {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                }
                JobsWorkflowTerminalReason::InterventionLimit => {
                    let unpublished: i64 = transaction
                        .query_one(
                            "SELECT COUNT(*)::bigint
                               FROM jobs_workflow_intervention_preparations
                              WHERE command_id = $1 AND published_at_ms IS NULL",
                            &[&command.id],
                        )?
                        .get(0);
                    if unpublished != 1 {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                }
                JobsWorkflowTerminalReason::RunnerFailed
                | JobsWorkflowTerminalReason::RunnerAmbiguous => {
                    require_no_open_runner_intervention_postgres_tx(&mut transaction, &command)?;
                }
            }
            let changed = transaction.execute(
                "INSERT INTO jobs_workflow_execution_finalizations (
                    command_id, account_id, request_id, payload_hmac_sha256,
                    outcome_kind, reason_code, open_intervention_id,
                    outcome_hmac_sha256, finalized_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(command_id) DO NOTHING",
                &[
                    &command.id,
                    &command.account_id,
                    &command.request_id,
                    &command.payload_hmac_sha256,
                    &outcome_kind,
                    &reason_code,
                    &input.open_intervention_id,
                    &outcome_hmac,
                    &input.now_ms,
                ],
            )?;
            let stored: String = transaction
                .query_one(
                    "SELECT outcome_hmac_sha256 FROM jobs_workflow_execution_finalizations
                      WHERE command_id = $1 FOR SHARE",
                    &[&command.id],
                )?
                .get(0);
            if !workflow_command_hmac_matches(&stored, &outcome_hmac) {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if changed == 1 {
                if let Some(intervention_id) = input.open_intervention_id.as_deref() {
                    close_exact_workflow_intervention_postgres_tx(
                        &mut transaction,
                        &command,
                        intervention_id,
                        input.now_ms,
                    )?;
                }
                terminalize_workflow_rows_postgres_tx(
                    &mut transaction,
                    &command,
                    outcome_kind,
                    input.now_ms,
                )?;
                mark_workflow_execution_terminal_postgres_tx(
                    &mut transaction,
                    &command,
                    input.now_ms,
                )?;
                let discarded_hidden_prompt = input.outcome
                    == JobsWorkflowTerminalOutcome::Failed(
                        JobsWorkflowTerminalReason::InterventionLimit,
                    )
                    && input.open_intervention_id.is_none()
                    && transaction.execute(
                        "DELETE FROM jobs_workflow_intervention_preparations
                          WHERE command_id = $1 AND published_at_ms IS NULL",
                        &[&command.id],
                    )? == 1;
                if input.outcome
                    == JobsWorkflowTerminalOutcome::Failed(
                        JobsWorkflowTerminalReason::InterventionLimit,
                    )
                    && !discarded_hidden_prompt
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
            }
            transaction.commit()?;
            Ok(JobsWorkflowExecutionFinalization {
                request_id: input.request_id.clone(),
                outcome: input.outcome,
                replayed: changed == 0,
            })
        }
    })
}

pub fn mark_jobs_workflow_execution_submitted(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    workflow_id: &str,
    now_ms: i64,
) -> Result<bool> {
    if !workflow_command_identifier(account_id, 128)
        || !workflow_command_identifier(application_id, 128)
        || !workflow_command_opaque_identifier(run_id, 128)
        || !workflow_command_opaque_identifier(workflow_id, 192)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (raw, state, job_id): (String, String, String) = transaction
                .query_row(
                    "SELECT application_json, state, job_id FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let application = parse_application_json(
                raw,
                application_id,
                &job_id,
                "submitted workflow application",
            )?;
            if state != "submitted"
                || application.state != state
                || application.run_id.as_deref() != Some(run_id)
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let command_id: Option<String> = transaction
                .query_row(
                    "SELECT id FROM jobs_workflow_commands
                      WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                        AND workflow_id = ?4 AND command_kind = 'start'",
                    params![account_id, application_id, run_id, workflow_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(command_id) = command_id else {
                transaction.commit()?;
                return Ok(false);
            };
            let authority: (String, String, String, String) = transaction
                .query_row(
                    "SELECT execution.start_command_id, execution.first_execution_run_id,
                            command.application_id, command.run_id
                       FROM jobs_workflow_executions execution
                       JOIN jobs_workflow_commands command
                         ON command.id = execution.start_command_id
                        AND command.account_id = execution.account_id
                      WHERE execution.account_id = ?1 AND execution.workflow_id = ?2",
                    params![account_id, workflow_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            if authority.2 != application_id
                || authority.3 != run_id
                || authority.0 != command_id
                || !workflow_command_opaque_identifier(&authority.1, 128)
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if transaction.execute(
                "UPDATE jobs_workflow_executions
                    SET lifecycle_state = 'terminal', terminal_at_ms = COALESCE(terminal_at_ms, ?1),
                        updated_at_ms = MAX(?1, updated_at_ms + 1)
                  WHERE account_id = ?2 AND workflow_id = ?3 AND start_command_id = ?4
                    AND first_execution_run_id = ?5
                    AND lifecycle_state IN ('accepted', 'running', 'terminal')",
                params![now_ms, account_id, workflow_id, authority.0, authority.1],
            )? != 1
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            transaction.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let row = transaction
                .query_opt(
                    "SELECT application_json, state, job_id FROM jobs_applications
                      WHERE account_id = $1 AND id = $2 FOR SHARE",
                    &[&account_id, &application_id],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let state: String = row.get(1);
            let job_id: String = row.get(2);
            let application = parse_application_json(
                row.get(0),
                application_id,
                &job_id,
                "submitted workflow application",
            )?;
            if state != "submitted"
                || application.state != state
                || application.run_id.as_deref() != Some(run_id)
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            let command = transaction.query_opt(
                "SELECT id FROM jobs_workflow_commands
                  WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                    AND workflow_id = $4 AND command_kind = 'start' FOR SHARE",
                &[&account_id, &application_id, &run_id, &workflow_id],
            )?;
            let Some(command) = command else {
                transaction.commit()?;
                return Ok(false);
            };
            let command_id: String = command.get(0);
            let authority = transaction
                .query_opt(
                    "SELECT execution.start_command_id, execution.first_execution_run_id,
                            command.application_id, command.run_id
                       FROM jobs_workflow_executions execution
                       JOIN jobs_workflow_commands command
                         ON command.id = execution.start_command_id
                        AND command.account_id = execution.account_id
                      WHERE execution.account_id = $1 AND execution.workflow_id = $2 FOR UPDATE",
                    &[&account_id, &workflow_id],
                )?
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            let start_command_id: String = authority.get(0);
            let first_run_id: String = authority.get(1);
            if authority.get::<_, String>(2) != application_id
                || authority.get::<_, String>(3) != run_id
                || start_command_id != command_id
                || !workflow_command_opaque_identifier(&first_run_id, 128)
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            if transaction.execute(
                "UPDATE jobs_workflow_executions
                    SET lifecycle_state = 'terminal', terminal_at_ms = COALESCE(terminal_at_ms, $1),
                        updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE account_id = $2 AND workflow_id = $3 AND start_command_id = $4
                    AND first_execution_run_id = $5
                    AND lifecycle_state IN ('accepted', 'running', 'terminal')",
                &[
                    &now_ms,
                    &account_id,
                    &workflow_id,
                    &start_command_id,
                    &first_run_id,
                ],
            )? != 1
            {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            transaction.commit()?;
            Ok(true)
        }
    })
}

pub fn prepare_jobs_workflow_cleanup(
    pool: &DbPool,
    input: &PrepareJobsWorkflowCleanup,
) -> Result<JobsWorkflowCleanupStatus> {
    if !workflow_command_identifier(&input.account_id, 128)
        || !(1..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.generation)
        // A caller assertion is not proof that pre-v2 Temporal workflows were
        // inventoried. Until a signed, paginated legacy-inventory receipt is
        // persisted, cleanup may freeze/drain work but must remain fail closed.
        || input.legacy_reconciled
        || !(1..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.legacy_unresolved_count)
        || !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms)
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(existing) = transaction
                .query_row(
                    "SELECT generation, legacy_reconciled, legacy_unresolved_count
                       FROM jobs_workflow_cleanup_generations
                      WHERE account_id = ?1",
                    params![input.account_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()?
            {
                if existing.0 != input.generation
                    || existing.1 != input.legacy_reconciled
                    || existing.2 != input.legacy_unresolved_count
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                let status = workflow_cleanup_status_sqlite_tx(
                    &transaction,
                    &input.account_id,
                    input.generation,
                    true,
                )?;
                transaction.commit()?;
                return Ok(status);
            }
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &transaction,
                &input.account_id,
            )?;
            cancel_never_delivered_workflow_commands_sqlite_tx(
                &transaction,
                &input.account_id,
                input.now_ms,
            )?;
            let (target_set_hmac, targets) = workflow_cleanup_targets_sqlite_tx(
                &transaction,
                &input.account_id,
                input.generation,
                input.legacy_reconciled,
                input.legacy_unresolved_count,
            )?;
            let target_count = targets.len() as i64;
            transaction.execute(
                "INSERT INTO jobs_workflow_cleanup_generations (
                    account_id, generation, state, target_set_hmac_sha256, target_count,
                    legacy_reconciled, legacy_unresolved_count, frozen_at_ms,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, 'frozen', ?3, ?4, ?5, ?6, ?7, ?7, ?7)",
                params![
                    input.account_id,
                    input.generation,
                    target_set_hmac,
                    target_count,
                    i64::from(input.legacy_reconciled),
                    input.legacy_unresolved_count,
                    input.now_ms
                ],
            )?;
            for target in &targets {
                let target_state = if target.first_execution_run_id.is_some() {
                    "cleanup_required"
                } else if target.command_state == "delivery_unknown" {
                    "identity_reconcile"
                } else {
                    "delivery_drain"
                };
                transaction.execute(
                    "INSERT INTO jobs_workflow_cleanup_targets (
                        account_id, generation, target_set_hmac_sha256, workflow_id,
                        start_command_id, start_request_id, start_payload_hmac_sha256,
                        first_execution_run_id, target_state, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                    params![
                        input.account_id,
                        input.generation,
                        target_set_hmac,
                        target.workflow_id,
                        target.command_id,
                        target.request_id,
                        target.payload_hmac_sha256,
                        target.first_execution_run_id,
                        target_state,
                        input.now_ms
                    ],
                )?;
            }
            let bound_count = transaction.execute(
                "UPDATE jobs_workflow_executions
                    SET deletion_target_generation = ?1, deletion_target_hmac_sha256 = ?2,
                        updated_at_ms = MAX(?3, updated_at_ms + 1)
                  WHERE account_id = ?4 AND deletion_target_generation IS NULL",
                params![
                    input.generation,
                    target_set_hmac,
                    input.now_ms,
                    input.account_id
                ],
            )? as i64;
            let expected_bound = targets
                .iter()
                .filter(|target| target.first_execution_run_id.is_some())
                .count() as i64;
            if bound_count != expected_bound {
                anyhow::bail!("workflow cleanup target set changed while it was frozen")
            }
            let status = workflow_cleanup_status_sqlite_tx(
                &transaction,
                &input.account_id,
                input.generation,
                false,
            )?;
            transaction.commit()?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            if let Some(row) = transaction.query_opt(
                "SELECT generation, legacy_reconciled, legacy_unresolved_count
                   FROM jobs_workflow_cleanup_generations
                  WHERE account_id = $1 FOR UPDATE",
                &[&input.account_id],
            )? {
                let existing_generation: i64 = row.get(0);
                if existing_generation != input.generation
                    || row.get::<_, bool>(1) != input.legacy_reconciled
                    || row.get::<_, i64>(2) != input.legacy_unresolved_count
                {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                let status = workflow_cleanup_status_postgres_tx(
                    &mut transaction,
                    &input.account_id,
                    input.generation,
                    true,
                )?;
                transaction.commit()?;
                return Ok(status);
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut transaction,
                &input.account_id,
            )?;
            cancel_never_delivered_workflow_commands_postgres_tx(
                &mut transaction,
                &input.account_id,
                input.now_ms,
            )?;
            let (target_set_hmac, targets) = workflow_cleanup_targets_postgres_tx(
                &mut transaction,
                &input.account_id,
                input.generation,
                input.legacy_reconciled,
                input.legacy_unresolved_count,
            )?;
            let target_count = targets.len() as i64;
            transaction.execute(
                "INSERT INTO jobs_workflow_cleanup_generations (
                    account_id, generation, state, target_set_hmac_sha256, target_count,
                    legacy_reconciled, legacy_unresolved_count, frozen_at_ms,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, 'frozen', $3, $4, $5, $6, $7, $7, $7)",
                &[
                    &input.account_id,
                    &input.generation,
                    &target_set_hmac,
                    &target_count,
                    &input.legacy_reconciled,
                    &input.legacy_unresolved_count,
                    &input.now_ms,
                ],
            )?;
            for target in &targets {
                let target_state = if target.first_execution_run_id.is_some() {
                    "cleanup_required"
                } else if target.command_state == "delivery_unknown" {
                    "identity_reconcile"
                } else {
                    "delivery_drain"
                };
                transaction.execute(
                    "INSERT INTO jobs_workflow_cleanup_targets (
                        account_id, generation, target_set_hmac_sha256, workflow_id,
                        start_command_id, start_request_id, start_payload_hmac_sha256,
                        first_execution_run_id, target_state, created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)",
                    &[
                        &input.account_id,
                        &input.generation,
                        &target_set_hmac,
                        &target.workflow_id,
                        &target.command_id,
                        &target.request_id,
                        &target.payload_hmac_sha256,
                        &target.first_execution_run_id,
                        &target_state,
                        &input.now_ms,
                    ],
                )?;
            }
            let bound_count = transaction.execute(
                "UPDATE jobs_workflow_executions
                    SET deletion_target_generation = $1, deletion_target_hmac_sha256 = $2,
                        updated_at_ms = GREATEST($3, updated_at_ms + 1)
                  WHERE account_id = $4 AND deletion_target_generation IS NULL",
                &[
                    &input.generation,
                    &target_set_hmac,
                    &input.now_ms,
                    &input.account_id,
                ],
            )? as i64;
            let expected_bound = targets
                .iter()
                .filter(|target| target.first_execution_run_id.is_some())
                .count() as i64;
            if bound_count != expected_bound {
                anyhow::bail!("workflow cleanup target set changed while it was frozen")
            }
            let status = workflow_cleanup_status_postgres_tx(
                &mut transaction,
                &input.account_id,
                input.generation,
                false,
            )?;
            transaction.commit()?;
            Ok(status)
        }
    })
}

pub fn get_jobs_workflow_cleanup_status(
    pool: &DbPool,
    account_id: &str,
    generation: i64,
) -> Result<Option<JobsWorkflowCleanupStatus>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            if connection
                .query_row(
                    "SELECT 1 FROM jobs_workflow_cleanup_generations
                      WHERE account_id = ?1 AND generation = ?2",
                    params![account_id, generation],
                    |_| Ok(()),
                )
                .optional()?
                .is_none()
            {
                return Ok(None);
            }
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let status =
                workflow_cleanup_status_sqlite_tx(&transaction, account_id, generation, true)?;
            transaction.commit()?;
            Ok(Some(status))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            if transaction
                .query_opt(
                    "SELECT 1 FROM jobs_workflow_cleanup_generations
                      WHERE account_id = $1 AND generation = $2",
                    &[&account_id, &generation],
                )?
                .is_none()
            {
                transaction.commit()?;
                return Ok(None);
            }
            let status = workflow_cleanup_status_postgres_tx(
                &mut transaction,
                account_id,
                generation,
                true,
            )?;
            transaction.commit()?;
            Ok(Some(status))
        }
    })
}

fn validate_jobs_workflow_cleanup_evidence(
    lease: &JobsWorkflowCleanupLease,
    input: &CompleteJobsWorkflowCleanupTarget,
) -> Result<()> {
    let Some(object) = input.evidence.as_object() else {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    };
    let mut allowed = vec![
        "schemaVersion",
        "provider",
        "cleanupRequestId",
        "cleanupFence",
        "requestId",
        "workflowId",
        "payloadHmacSha256",
        "observationKind",
        "status",
        "firstExecutionRunId",
        "observedAtMs",
    ];
    let expected_status = match input.observation_kind {
        JobsWorkflowCleanupObservationKind::IdentityConfirmed => "found",
        JobsWorkflowCleanupObservationKind::IdentityConflict => "identity_conflict",
        JobsWorkflowCleanupObservationKind::TerminationRequested
        | JobsWorkflowCleanupObservationKind::HistoryDeleteRequested => "requested",
        JobsWorkflowCleanupObservationKind::TerminationConfirmed => "terminated",
        JobsWorkflowCleanupObservationKind::HistoryDeleteConfirmed => "deleted",
        JobsWorkflowCleanupObservationKind::AbsenceProved => "complete",
    };
    if matches!(
        input.observation_kind,
        JobsWorkflowCleanupObservationKind::IdentityConfirmed
            | JobsWorkflowCleanupObservationKind::IdentityConflict
            | JobsWorkflowCleanupObservationKind::AbsenceProved
    ) {
        allowed.extend(["workflowType", "taskQueue", "memo"]);
    }
    if input.observation_kind == JobsWorkflowCleanupObservationKind::AbsenceProved {
        allowed.extend([
            "outcome",
            "reason",
            "enumerationComplete",
            "nextPageToken",
            "runs",
        ]);
    }
    if object.len() != allowed.len()
        || object.keys().any(|key| !allowed.contains(&key.as_str()))
        || object.get("schemaVersion").and_then(Value::as_i64) != Some(2)
        || object.get("provider").and_then(Value::as_str) != Some("temporal")
        || object.get("cleanupRequestId").and_then(Value::as_str)
            != Some(lease.cleanup_request_id.as_str())
        || object.get("cleanupFence").and_then(Value::as_i64) != Some(lease.fence)
        || object.get("requestId").and_then(Value::as_str) != Some(lease.start_request_id.as_str())
        || object.get("workflowId").and_then(Value::as_str) != Some(lease.workflow_id.as_str())
        || object.get("payloadHmacSha256").and_then(Value::as_str)
            != Some(lease.start_payload_hmac_sha256.as_str())
        || object.get("observationKind").and_then(Value::as_str)
            != Some(input.observation_kind.as_str())
        || object.get("status").and_then(Value::as_str) != Some(expected_status)
        || object.get("observedAtMs").and_then(Value::as_i64) != Some(input.now_ms)
        || object.get("firstExecutionRunId").and_then(Value::as_str)
            != input.first_execution_run_id.as_deref()
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    if matches!(
        input.observation_kind,
        JobsWorkflowCleanupObservationKind::IdentityConfirmed
            | JobsWorkflowCleanupObservationKind::IdentityConflict
            | JobsWorkflowCleanupObservationKind::AbsenceProved
    ) {
        let memo = object.get("memo").and_then(Value::as_object);
        if object.get("workflowType").and_then(Value::as_str) != Some("applicationWorkflowV2")
            || object
                .get("taskQueue")
                .and_then(Value::as_str)
                .is_none_or(|value| !workflow_command_identifier(value, 128))
            || memo.is_none_or(|memo| {
                memo.len() != 4
                    || memo.get("schemaVersion").and_then(Value::as_i64) != Some(2)
                    || memo.get("requestId").and_then(Value::as_str)
                        != Some(lease.start_request_id.as_str())
                    || memo.get("workflowId").and_then(Value::as_str)
                        != Some(lease.workflow_id.as_str())
                    || memo.get("payloadDigest").and_then(Value::as_str)
                        != Some(lease.start_payload_hmac_sha256.as_str())
            })
        {
            return Err(JobsWorkflowCommandError::IdentityConflict.into());
        }
    }
    if input.observation_kind == JobsWorkflowCleanupObservationKind::AbsenceProved {
        let runs = object.get("runs").and_then(Value::as_array);
        if object.get("outcome").and_then(Value::as_str) != Some("complete")
            || object.get("reason").and_then(Value::as_str) != Some("absence_proved")
            || object.get("enumerationComplete").and_then(Value::as_bool) != Some(true)
            || !object.get("nextPageToken").is_some_and(Value::is_null)
            || runs.is_none_or(|runs| runs.len() > 1_024)
        {
            return Err(JobsWorkflowCommandError::InvalidRequest.into());
        }
        let runs = runs.expect("absence runs checked");
        let mut seen = BTreeSet::new();
        for run in runs {
            let Some(run) = run.as_object() else {
                return Err(JobsWorkflowCommandError::InvalidRequest.into());
            };
            let run_id = run.get("runId").and_then(Value::as_str);
            if run.len() != 4
                || run.keys().any(|key| {
                    !["runId", "describe", "history", "visibility"].contains(&key.as_str())
                })
                || run_id.is_none_or(|value| !workflow_command_opaque_identifier(value, 128))
                || run.get("describe").and_then(Value::as_str) != Some("not_found")
                || run.get("history").and_then(Value::as_str) != Some("not_found")
                || run.get("visibility").and_then(Value::as_str) != Some("not_found")
                || !seen.insert(run_id.expect("run id checked"))
            {
                return Err(JobsWorkflowCommandError::InvalidRequest.into());
            }
        }
        match lease.first_execution_run_id.as_deref() {
            Some(first_run_id) if runs.is_empty() || !seen.contains(first_run_id) => {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            None if !runs.is_empty() => {
                return Err(JobsWorkflowCommandError::IdentityConflict.into());
            }
            _ => {}
        }
    }
    canonical_workflow_command_bytes(
        &input.evidence,
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
        "workflow cleanup evidence",
    )?;
    Ok(())
}

fn jobs_workflow_cleanup_evidence_hmac(
    lease: &JobsWorkflowCleanupLease,
    input: &CompleteJobsWorkflowCleanupTarget,
) -> Result<String> {
    workflow_command_hmac(
        "cleanup-observation",
        &json!({
            "accountId": lease.account_id,
            "generation": lease.generation,
            "targetSetHmacSha256": lease.target_set_hmac_sha256,
            "workflowId": lease.workflow_id,
            "startCommandId": lease.start_command_id,
            "startRequestId": lease.start_request_id,
            "startPayloadHmacSha256": lease.start_payload_hmac_sha256,
            "cleanupFence": lease.fence,
            "cleanupRequestId": lease.cleanup_request_id,
            "observationKind": input.observation_kind.as_str(),
            "firstExecutionRunId": input.first_execution_run_id,
            "evidence": input.evidence,
        }),
        WORKFLOW_COMMAND_REQUEST_MAX_BYTES,
    )
}

fn validate_jobs_workflow_cleanup_completion(
    lease: &JobsWorkflowCleanupLease,
    input: &CompleteJobsWorkflowCleanupTarget,
) -> Result<()> {
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&input.now_ms)
        || !workflow_command_identifier(&lease.account_id, 128)
        || !workflow_command_opaque_identifier(&lease.workflow_id, 192)
        || !workflow_command_identifier(&lease.start_command_id, 128)
        || !workflow_command_opaque_identifier(&lease.start_request_id, 128)
        || lease.start_payload_hmac_sha256.len() != 64
        || !(1..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&lease.generation)
        || !(1..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&lease.fence)
        || !workflow_command_opaque_identifier(&lease.cleanup_request_id, 128)
        || !workflow_command_identifier(&lease.lease_owner, 128)
        || lease.lease_token.is_empty()
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    if input
        .first_execution_run_id
        .as_deref()
        .is_some_and(|value| !workflow_command_opaque_identifier(value, 128))
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let run_matches = match input.observation_kind {
        JobsWorkflowCleanupObservationKind::IdentityConfirmed => {
            lease.first_execution_run_id.is_none() && input.first_execution_run_id.is_some()
        }
        JobsWorkflowCleanupObservationKind::IdentityConflict => {
            input.first_execution_run_id != lease.first_execution_run_id
        }
        JobsWorkflowCleanupObservationKind::AbsenceProved => {
            input.first_execution_run_id == lease.first_execution_run_id
        }
        _ => {
            lease.first_execution_run_id.is_some()
                && input.first_execution_run_id == lease.first_execution_run_id
        }
    };
    if !run_matches {
        return Err(JobsWorkflowCommandError::IdentityConflict.into());
    }
    validate_jobs_workflow_cleanup_evidence(lease, input)
}

pub fn claim_jobs_workflow_cleanup_target(
    pool: &DbPool,
    owner_id: &str,
    now_ms: i64,
    lease_ms: i64,
) -> Result<Option<JobsWorkflowCleanupLease>> {
    validate_workflow_command_lease_input(owner_id, now_ms, lease_ms)?;
    let lease_expires_at_ms = now_ms + lease_ms;
    let lease_token = workflow_command_random_lease_token();
    let lease_hash = workflow_command_lease_token_sha256(&lease_token);
    let cleanup_request_id = new_jobs_workflow_cleanup_request_id();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute(
                "UPDATE jobs_workflow_cleanup_targets
                    SET target_state = 'identity_reconcile',
                        updated_at_ms = MAX(?1, updated_at_ms + 1)
                  WHERE target_state = 'delivery_drain' AND EXISTS (
                    SELECT 1 FROM jobs_workflow_commands command
                     WHERE command.id = jobs_workflow_cleanup_targets.start_command_id
                       AND command.account_id = jobs_workflow_cleanup_targets.account_id
                       AND command.request_id = jobs_workflow_cleanup_targets.start_request_id
                       AND command.payload_hmac_sha256 =
                           jobs_workflow_cleanup_targets.start_payload_hmac_sha256
                       AND command.state = 'delivery_unknown'
                       AND command.lease_owner IS NULL
                  )",
                params![now_ms],
            )?;
            let target = transaction
                .query_row(
                    "SELECT account_id, workflow_id, start_command_id, start_request_id,
                            start_payload_hmac_sha256, first_execution_run_id, generation,
                            target_set_hmac_sha256, target_state, fence
                       FROM jobs_workflow_cleanup_targets
                      WHERE target_state NOT IN (
                        'delivery_drain', 'absence_proved', 'identity_conflict'
                      )
                        AND (lease_owner IS NULL OR lease_expires_at_ms <= ?1)
                      ORDER BY updated_at_ms, account_id, workflow_id LIMIT 1",
                    params![now_ms],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, i64>(9)?,
                        ))
                    },
                )
                .optional()?;
            let Some(target) = target else {
                transaction.commit()?;
                return Ok(None);
            };
            let fence = target
                .9
                .checked_add(1)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            if transaction.execute(
                "UPDATE jobs_workflow_cleanup_targets
                    SET fence = ?1, cleanup_request_id = ?2, lease_owner = ?3,
                        lease_token_sha256 = ?4, lease_expires_at_ms = ?5,
                        updated_at_ms = MAX(?6, updated_at_ms + 1)
                  WHERE account_id = ?7 AND generation = ?8 AND workflow_id = ?9
                    AND fence = ?10 AND target_state NOT IN (
                      'delivery_drain', 'absence_proved', 'identity_conflict'
                    )
                    AND (lease_owner IS NULL OR lease_expires_at_ms <= ?6)",
                params![
                    fence,
                    cleanup_request_id,
                    owner_id,
                    lease_hash,
                    lease_expires_at_ms,
                    now_ms,
                    target.0,
                    target.6,
                    target.1,
                    target.9
                ],
            )? != 1
            {
                anyhow::bail!("workflow cleanup target changed while being claimed")
            }
            transaction.execute(
                "UPDATE jobs_workflow_cleanup_generations
                    SET state = 'cleaning', updated_at_ms = MAX(?1, updated_at_ms + 1)
                  WHERE account_id = ?2 AND generation = ?3 AND state = 'frozen'",
                params![now_ms, target.0, target.6],
            )?;
            transaction.commit()?;
            Ok(Some(JobsWorkflowCleanupLease {
                account_id: target.0,
                workflow_id: target.1,
                start_command_id: target.2,
                start_request_id: target.3,
                start_payload_hmac_sha256: target.4,
                first_execution_run_id: target.5,
                generation: target.6,
                target_set_hmac_sha256: target.7,
                cleanup_state: target.8,
                fence,
                cleanup_request_id,
                lease_owner: owner_id.to_string(),
                lease_token,
                lease_expires_at_ms,
            }))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            transaction.execute(
                "UPDATE jobs_workflow_cleanup_targets target
                    SET target_state = 'identity_reconcile',
                        updated_at_ms = GREATEST($1, target.updated_at_ms + 1)
                   FROM jobs_workflow_commands command
                  WHERE target.target_state = 'delivery_drain'
                    AND command.id = target.start_command_id
                    AND command.account_id = target.account_id
                    AND command.request_id = target.start_request_id
                    AND command.payload_hmac_sha256 = target.start_payload_hmac_sha256
                    AND command.state = 'delivery_unknown'
                    AND command.lease_owner IS NULL",
                &[&now_ms],
            )?;
            let target = transaction.query_opt(
                "SELECT account_id, workflow_id, start_command_id, start_request_id,
                        start_payload_hmac_sha256, first_execution_run_id, generation,
                        target_set_hmac_sha256, target_state, fence
                   FROM jobs_workflow_cleanup_targets
                  WHERE target_state NOT IN (
                    'delivery_drain', 'absence_proved', 'identity_conflict'
                  )
                    AND (lease_owner IS NULL OR lease_expires_at_ms <= $1)
                  ORDER BY updated_at_ms, account_id, workflow_id
                  FOR UPDATE SKIP LOCKED LIMIT 1",
                &[&now_ms],
            )?;
            let Some(target) = target else {
                transaction.commit()?;
                return Ok(None);
            };
            let account_id: String = target.get(0);
            let workflow_id: String = target.get(1);
            let generation: i64 = target.get(6);
            let old_fence: i64 = target.get(9);
            let fence = old_fence
                .checked_add(1)
                .ok_or(JobsWorkflowCommandError::InvalidState)?;
            if transaction.execute(
                "UPDATE jobs_workflow_cleanup_targets
                    SET fence = $1, cleanup_request_id = $2, lease_owner = $3,
                        lease_token_sha256 = $4, lease_expires_at_ms = $5,
                        updated_at_ms = GREATEST($6, updated_at_ms + 1)
                  WHERE account_id = $7 AND generation = $8 AND workflow_id = $9
                    AND fence = $10 AND target_state NOT IN (
                      'delivery_drain', 'absence_proved', 'identity_conflict'
                    )
                    AND (lease_owner IS NULL OR lease_expires_at_ms <= $6)",
                &[
                    &fence,
                    &cleanup_request_id,
                    &owner_id,
                    &lease_hash,
                    &lease_expires_at_ms,
                    &now_ms,
                    &account_id,
                    &generation,
                    &workflow_id,
                    &old_fence,
                ],
            )? != 1
            {
                anyhow::bail!("workflow cleanup target changed while being claimed")
            }
            transaction.execute(
                "UPDATE jobs_workflow_cleanup_generations
                    SET state = 'cleaning', updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE account_id = $2 AND generation = $3 AND state = 'frozen'",
                &[&now_ms, &account_id, &generation],
            )?;
            let lease = JobsWorkflowCleanupLease {
                account_id,
                workflow_id,
                start_command_id: target.get(2),
                start_request_id: target.get(3),
                start_payload_hmac_sha256: target.get(4),
                first_execution_run_id: target.get(5),
                generation,
                target_set_hmac_sha256: target.get(7),
                cleanup_state: target.get(8),
                fence,
                cleanup_request_id,
                lease_owner: owner_id.to_string(),
                lease_token,
                lease_expires_at_ms,
            };
            transaction.commit()?;
            Ok(Some(lease))
        }
    })
}

pub fn complete_jobs_workflow_cleanup_target(
    pool: &DbPool,
    lease: &JobsWorkflowCleanupLease,
    input: &CompleteJobsWorkflowCleanupTarget,
) -> Result<JobsWorkflowCleanupObservationReceipt> {
    validate_jobs_workflow_cleanup_completion(lease, input)?;
    let evidence_hmac = jobs_workflow_cleanup_evidence_hmac(lease, input)?;
    let observation_kind = input.observation_kind.as_str();
    let token_hash = workflow_command_lease_token_sha256(&lease.lease_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(existing_hmac) = transaction
                .query_row(
                    "SELECT evidence_hmac_sha256
                       FROM jobs_workflow_execution_cleanup_observations
                      WHERE account_id = ?1 AND workflow_id = ?2 AND generation = ?3
                        AND cleanup_fence = ?4 AND cleanup_request_id = ?5
                        AND observation_kind = ?6",
                    params![
                        lease.account_id,
                        lease.workflow_id,
                        lease.generation,
                        lease.fence,
                        lease.cleanup_request_id,
                        observation_kind
                    ],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            {
                if !workflow_command_hmac_matches(&existing_hmac, &evidence_hmac) {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                transaction.commit()?;
                return Ok(JobsWorkflowCleanupObservationReceipt {
                    account_id: lease.account_id.clone(),
                    workflow_id: lease.workflow_id.clone(),
                    generation: lease.generation,
                    observation_kind: input.observation_kind,
                    replayed: true,
                });
            }
            let stored = transaction.query_row(
                "SELECT start_request_id, start_payload_hmac_sha256,
                        first_execution_run_id, target_state, fence, cleanup_request_id, lease_owner,
                        lease_token_sha256, lease_expires_at_ms
                   FROM jobs_workflow_cleanup_targets
                  WHERE account_id = ?1 AND generation = ?2 AND workflow_id = ?3
                    AND target_set_hmac_sha256 = ?4 AND start_command_id = ?5",
                params![
                    lease.account_id,
                    lease.generation,
                    lease.workflow_id,
                    lease.target_set_hmac_sha256,
                    lease.start_command_id
                ],
                sqlite_workflow_cleanup_lease_row,
            )?;
            if stored.start_request_id != lease.start_request_id
                || !workflow_command_hmac_matches(
                    &stored.start_payload_hmac_sha256,
                    &lease.start_payload_hmac_sha256,
                )
                || stored.first_execution_run_id != lease.first_execution_run_id
                || stored.target_state != lease.cleanup_state
                || stored.fence != lease.fence
                || stored.cleanup_request_id.as_deref() != Some(lease.cleanup_request_id.as_str())
                || stored.lease_owner.as_deref() != Some(lease.lease_owner.as_str())
                || stored
                    .lease_token_sha256
                    .as_deref()
                    .is_none_or(|value| !workflow_command_hmac_matches(value, &token_hash))
                || stored.lease_expires_at_ms != Some(lease.lease_expires_at_ms)
                || lease.lease_expires_at_ms < input.now_ms
            {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            transaction.execute(
                "INSERT INTO jobs_workflow_execution_cleanup_observations (
                    id, account_id, workflow_id, generation, target_set_hmac_sha256,
                    observation_kind, observed_execution_run_id, cleanup_fence,
                    cleanup_request_id,
                    evidence_hmac_sha256, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    new_jobs_workflow_cleanup_observation_id(),
                    lease.account_id,
                    lease.workflow_id,
                    lease.generation,
                    lease.target_set_hmac_sha256,
                    observation_kind,
                    input.first_execution_run_id,
                    lease.fence,
                    lease.cleanup_request_id,
                    evidence_hmac,
                    input.now_ms
                ],
            )?;
            let next_state;
            let mut next_run_id = stored.first_execution_run_id;
            match input.observation_kind {
                JobsWorkflowCleanupObservationKind::IdentityConfirmed => {
                    let run_id = input
                        .first_execution_run_id
                        .as_ref()
                        .ok_or(JobsWorkflowCommandError::IdentityConflict)?;
                    transaction.execute(
                        "INSERT INTO jobs_workflow_executions (
                            account_id, workflow_id, start_command_id,
                            first_execution_run_id, lifecycle_state, cleanup_state,
                            deletion_target_generation, deletion_target_hmac_sha256,
                            created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, 'running', 'required', ?5, ?6, ?7, ?7)
                         ON CONFLICT(account_id, workflow_id) DO NOTHING",
                        params![
                            lease.account_id,
                            lease.workflow_id,
                            lease.start_command_id,
                            run_id,
                            lease.generation,
                            lease.target_set_hmac_sha256,
                            input.now_ms
                        ],
                    )?;
                    let authority: (String, String) = transaction.query_row(
                        "SELECT start_command_id, first_execution_run_id
                           FROM jobs_workflow_executions
                          WHERE account_id = ?1 AND workflow_id = ?2",
                        params![lease.account_id, lease.workflow_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )?;
                    if authority.0 != lease.start_command_id || authority.1 != *run_id {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                    next_run_id = Some(run_id.clone());
                    next_state = "cleanup_required".to_string();
                }
                JobsWorkflowCleanupObservationKind::IdentityConflict => {
                    next_state = "identity_conflict".to_string();
                }
                JobsWorkflowCleanupObservationKind::TerminationRequested => {
                    next_state = "termination_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET lifecycle_state = 'termination_requested',
                                termination_requested_at_ms = ?1,
                                cleanup_state = 'termination_pending',
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE account_id = ?2 AND workflow_id = ?3
                            AND first_execution_run_id = ?4",
                        params![
                            input.now_ms,
                            lease.account_id,
                            lease.workflow_id,
                            input.first_execution_run_id
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::TerminationConfirmed => {
                    next_state = "history_delete_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET lifecycle_state = 'terminated', terminated_at_ms = ?1,
                                cleanup_state = 'history_delete_pending',
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE account_id = ?2 AND workflow_id = ?3
                            AND first_execution_run_id = ?4",
                        params![
                            input.now_ms,
                            lease.account_id,
                            lease.workflow_id,
                            input.first_execution_run_id
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::HistoryDeleteRequested => {
                    next_state = "history_delete_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET cleanup_state = 'history_delete_pending',
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE account_id = ?2 AND workflow_id = ?3
                            AND first_execution_run_id = ?4",
                        params![
                            input.now_ms,
                            lease.account_id,
                            lease.workflow_id,
                            input.first_execution_run_id
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::HistoryDeleteConfirmed => {
                    next_state = "history_delete_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET cleanup_state = 'history_deleted', history_deleted_at_ms = ?1,
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE account_id = ?2 AND workflow_id = ?3
                            AND first_execution_run_id = ?4",
                        params![
                            input.now_ms,
                            lease.account_id,
                            lease.workflow_id,
                            input.first_execution_run_id
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::AbsenceProved => {
                    next_state = "absence_proved".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET lifecycle_state = 'absent', cleanup_state = 'absence_proved',
                                absence_proved_at_ms = ?1,
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE account_id = ?2 AND workflow_id = ?3
                            AND first_execution_run_id IS ?4",
                        params![
                            input.now_ms,
                            lease.account_id,
                            lease.workflow_id,
                            lease.first_execution_run_id
                        ],
                    )?;
                    let command = transaction.query_row(
                        &format!(
                            "SELECT {WORKFLOW_COMMAND_SELECT}
                               FROM jobs_workflow_commands WHERE id = ?1"
                        ),
                        params![lease.start_command_id],
                        sqlite_workflow_command_row,
                    )?;
                    terminalize_local_workflow_authority_after_absence_sqlite_tx(
                        &transaction,
                        &command,
                        input.now_ms,
                    )?;
                }
            }
            let absence_at = (next_state == "absence_proved").then_some(input.now_ms);
            if transaction.execute(
                "UPDATE jobs_workflow_cleanup_targets
                    SET first_execution_run_id = ?1, target_state = ?2,
                        absence_proved_at_ms = COALESCE(absence_proved_at_ms, ?3),
                        lease_owner = NULL, lease_token_sha256 = NULL,
                        lease_expires_at_ms = NULL,
                        updated_at_ms = MAX(?4, updated_at_ms + 1)
                  WHERE account_id = ?5 AND generation = ?6 AND workflow_id = ?7
                    AND fence = ?8 AND lease_owner = ?9 AND lease_token_sha256 = ?10",
                params![
                    next_run_id,
                    next_state,
                    absence_at,
                    input.now_ms,
                    lease.account_id,
                    lease.generation,
                    lease.workflow_id,
                    lease.fence,
                    lease.lease_owner,
                    token_hash
                ],
            )? != 1
            {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            transaction.execute(
                "UPDATE jobs_workflow_cleanup_generations
                    SET state = 'complete', completed_at_ms = ?1,
                        updated_at_ms = MAX(?1, updated_at_ms + 1)
                  WHERE account_id = ?2 AND generation = ?3
                    AND legacy_reconciled = 1 AND legacy_unresolved_count = 0
                    AND target_count = (
                      SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
                       WHERE target.account_id = ?2 AND target.generation = ?3
                         AND target.target_set_hmac_sha256 = ?4
                         AND target.target_state = 'absence_proved'
                    )",
                params![
                    input.now_ms,
                    lease.account_id,
                    lease.generation,
                    lease.target_set_hmac_sha256
                ],
            )?;
            transaction.commit()?;
            Ok(JobsWorkflowCleanupObservationReceipt {
                account_id: lease.account_id.clone(),
                workflow_id: lease.workflow_id.clone(),
                generation: lease.generation,
                observation_kind: input.observation_kind,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            if let Some(row) = transaction.query_opt(
                "SELECT evidence_hmac_sha256
                   FROM jobs_workflow_execution_cleanup_observations
                  WHERE account_id = $1 AND workflow_id = $2 AND generation = $3
                    AND cleanup_fence = $4 AND cleanup_request_id = $5
                    AND observation_kind = $6",
                &[
                    &lease.account_id,
                    &lease.workflow_id,
                    &lease.generation,
                    &lease.fence,
                    &lease.cleanup_request_id,
                    &observation_kind,
                ],
            )? {
                let existing_hmac: String = row.get(0);
                if !workflow_command_hmac_matches(&existing_hmac, &evidence_hmac) {
                    return Err(JobsWorkflowCommandError::IdentityConflict.into());
                }
                transaction.commit()?;
                return Ok(JobsWorkflowCleanupObservationReceipt {
                    account_id: lease.account_id.clone(),
                    workflow_id: lease.workflow_id.clone(),
                    generation: lease.generation,
                    observation_kind: input.observation_kind,
                    replayed: true,
                });
            }
            let row = transaction.query_one(
                "SELECT start_request_id, start_payload_hmac_sha256,
                        first_execution_run_id, target_state, fence, cleanup_request_id, lease_owner,
                        lease_token_sha256, lease_expires_at_ms
                   FROM jobs_workflow_cleanup_targets
                  WHERE account_id = $1 AND generation = $2 AND workflow_id = $3
                    AND target_set_hmac_sha256 = $4 AND start_command_id = $5
                  FOR UPDATE",
                &[
                    &lease.account_id,
                    &lease.generation,
                    &lease.workflow_id,
                    &lease.target_set_hmac_sha256,
                    &lease.start_command_id,
                ],
            )?;
            let stored_request: String = row.get(0);
            let stored_payload: String = row.get(1);
            let stored_run: Option<String> = row.get(2);
            let stored_state: String = row.get(3);
            let stored_fence: i64 = row.get(4);
            let stored_cleanup_request_id: Option<String> = row.get(5);
            let stored_owner: Option<String> = row.get(6);
            let stored_token: Option<String> = row.get(7);
            let stored_expiry: Option<i64> = row.get(8);
            if stored_request != lease.start_request_id
                || !workflow_command_hmac_matches(&stored_payload, &lease.start_payload_hmac_sha256)
                || stored_run != lease.first_execution_run_id
                || stored_state != lease.cleanup_state
                || stored_fence != lease.fence
                || stored_cleanup_request_id.as_deref() != Some(lease.cleanup_request_id.as_str())
                || stored_owner.as_deref() != Some(lease.lease_owner.as_str())
                || stored_token
                    .as_deref()
                    .is_none_or(|value| !workflow_command_hmac_matches(value, &token_hash))
                || stored_expiry != Some(lease.lease_expires_at_ms)
                || lease.lease_expires_at_ms < input.now_ms
            {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            transaction.execute(
                "INSERT INTO jobs_workflow_execution_cleanup_observations (
                    id, account_id, workflow_id, generation, target_set_hmac_sha256,
                    observation_kind, observed_execution_run_id, cleanup_fence,
                    cleanup_request_id,
                    evidence_hmac_sha256, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &new_jobs_workflow_cleanup_observation_id(),
                    &lease.account_id,
                    &lease.workflow_id,
                    &lease.generation,
                    &lease.target_set_hmac_sha256,
                    &observation_kind,
                    &input.first_execution_run_id,
                    &lease.fence,
                    &lease.cleanup_request_id,
                    &evidence_hmac,
                    &input.now_ms,
                ],
            )?;
            let mut next_run_id = stored_run;
            let next_state;
            match input.observation_kind {
                JobsWorkflowCleanupObservationKind::IdentityConfirmed => {
                    let run_id = input
                        .first_execution_run_id
                        .as_ref()
                        .ok_or(JobsWorkflowCommandError::IdentityConflict)?;
                    transaction.execute(
                        "INSERT INTO jobs_workflow_executions (
                            account_id, workflow_id, start_command_id,
                            first_execution_run_id, lifecycle_state, cleanup_state,
                            deletion_target_generation, deletion_target_hmac_sha256,
                            created_at_ms, updated_at_ms
                         ) VALUES ($1, $2, $3, $4, 'running', 'required', $5, $6, $7, $7)
                         ON CONFLICT(account_id, workflow_id) DO NOTHING",
                        &[
                            &lease.account_id,
                            &lease.workflow_id,
                            &lease.start_command_id,
                            &run_id,
                            &lease.generation,
                            &lease.target_set_hmac_sha256,
                            &input.now_ms,
                        ],
                    )?;
                    let authority = transaction.query_one(
                        "SELECT start_command_id, first_execution_run_id
                           FROM jobs_workflow_executions
                          WHERE account_id = $1 AND workflow_id = $2 FOR SHARE",
                        &[&lease.account_id, &lease.workflow_id],
                    )?;
                    if authority.get::<_, String>(0) != lease.start_command_id
                        || authority.get::<_, String>(1) != *run_id
                    {
                        return Err(JobsWorkflowCommandError::IdentityConflict.into());
                    }
                    next_run_id = Some(run_id.clone());
                    next_state = "cleanup_required".to_string();
                }
                JobsWorkflowCleanupObservationKind::IdentityConflict => {
                    next_state = "identity_conflict".to_string();
                }
                JobsWorkflowCleanupObservationKind::TerminationRequested => {
                    next_state = "termination_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET lifecycle_state = 'termination_requested',
                                termination_requested_at_ms = $1,
                                cleanup_state = 'termination_pending',
                                updated_at_ms = GREATEST($1, updated_at_ms + 1)
                          WHERE account_id = $2 AND workflow_id = $3
                            AND first_execution_run_id = $4",
                        &[
                            &input.now_ms,
                            &lease.account_id,
                            &lease.workflow_id,
                            &input.first_execution_run_id,
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::TerminationConfirmed => {
                    next_state = "history_delete_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET lifecycle_state = 'terminated', terminated_at_ms = $1,
                                cleanup_state = 'history_delete_pending',
                                updated_at_ms = GREATEST($1, updated_at_ms + 1)
                          WHERE account_id = $2 AND workflow_id = $3
                            AND first_execution_run_id = $4",
                        &[
                            &input.now_ms,
                            &lease.account_id,
                            &lease.workflow_id,
                            &input.first_execution_run_id,
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::HistoryDeleteRequested => {
                    next_state = "history_delete_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET cleanup_state = 'history_delete_pending',
                                updated_at_ms = GREATEST($1, updated_at_ms + 1)
                          WHERE account_id = $2 AND workflow_id = $3
                            AND first_execution_run_id = $4",
                        &[
                            &input.now_ms,
                            &lease.account_id,
                            &lease.workflow_id,
                            &input.first_execution_run_id,
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::HistoryDeleteConfirmed => {
                    next_state = "history_delete_pending".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET cleanup_state = 'history_deleted', history_deleted_at_ms = $1,
                                updated_at_ms = GREATEST($1, updated_at_ms + 1)
                          WHERE account_id = $2 AND workflow_id = $3
                            AND first_execution_run_id = $4",
                        &[
                            &input.now_ms,
                            &lease.account_id,
                            &lease.workflow_id,
                            &input.first_execution_run_id,
                        ],
                    )?;
                }
                JobsWorkflowCleanupObservationKind::AbsenceProved => {
                    next_state = "absence_proved".to_string();
                    transaction.execute(
                        "UPDATE jobs_workflow_executions
                            SET lifecycle_state = 'absent', cleanup_state = 'absence_proved',
                                absence_proved_at_ms = $1,
                                updated_at_ms = GREATEST($1, updated_at_ms + 1)
                          WHERE account_id = $2 AND workflow_id = $3
                            AND first_execution_run_id IS NOT DISTINCT FROM $4",
                        &[
                            &input.now_ms,
                            &lease.account_id,
                            &lease.workflow_id,
                            &lease.first_execution_run_id,
                        ],
                    )?;
                    let command_row = transaction.query_one(
                        &format!(
                            "SELECT {WORKFLOW_COMMAND_SELECT}
                               FROM jobs_workflow_commands WHERE id = $1 FOR SHARE"
                        ),
                        &[&lease.start_command_id],
                    )?;
                    let command = postgres_workflow_command_row(&command_row);
                    terminalize_local_workflow_authority_after_absence_postgres_tx(
                        &mut transaction,
                        &command,
                        input.now_ms,
                    )?;
                }
            }
            let absence_at = (next_state == "absence_proved").then_some(input.now_ms);
            if transaction.execute(
                "UPDATE jobs_workflow_cleanup_targets
                    SET first_execution_run_id = $1, target_state = $2,
                        absence_proved_at_ms = COALESCE(absence_proved_at_ms, $3),
                        lease_owner = NULL, lease_token_sha256 = NULL,
                        lease_expires_at_ms = NULL,
                        updated_at_ms = GREATEST($4, updated_at_ms + 1)
                  WHERE account_id = $5 AND generation = $6 AND workflow_id = $7
                    AND fence = $8 AND lease_owner = $9 AND lease_token_sha256 = $10",
                &[
                    &next_run_id,
                    &next_state,
                    &absence_at,
                    &input.now_ms,
                    &lease.account_id,
                    &lease.generation,
                    &lease.workflow_id,
                    &lease.fence,
                    &lease.lease_owner,
                    &token_hash,
                ],
            )? != 1
            {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            transaction.execute(
                "UPDATE jobs_workflow_cleanup_generations
                    SET state = 'complete', completed_at_ms = $1,
                        updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE account_id = $2 AND generation = $3
                    AND legacy_reconciled AND legacy_unresolved_count = 0
                    AND target_count = (
                      SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets target
                       WHERE target.account_id = $2 AND target.generation = $3
                         AND target.target_set_hmac_sha256 = $4
                         AND target.target_state = 'absence_proved'
                    )",
                &[
                    &input.now_ms,
                    &lease.account_id,
                    &lease.generation,
                    &lease.target_set_hmac_sha256,
                ],
            )?;
            transaction.commit()?;
            Ok(JobsWorkflowCleanupObservationReceipt {
                account_id: lease.account_id.clone(),
                workflow_id: lease.workflow_id.clone(),
                generation: lease.generation,
                observation_kind: input.observation_kind,
                replayed: false,
            })
        }
    })
}

pub(crate) fn require_jobs_workflow_cleanup_complete_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    generation: i64,
    target_set_hmac_sha256: &str,
) -> Result<()> {
    let complete: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_workflow_cleanup_generations generation
          WHERE generation.account_id = ?1 AND generation.generation = ?2
            AND generation.target_set_hmac_sha256 = ?3 AND generation.state = 'complete'
            AND generation.legacy_reconciled = 1
            AND generation.legacy_unresolved_count = 0
            AND generation.target_count = (
              SELECT COUNT(*) FROM jobs_workflow_cleanup_targets target
               WHERE target.account_id = ?1 AND target.generation = ?2
                 AND target.target_set_hmac_sha256 = ?3
                 AND target.target_state = 'absence_proved'
            )",
        params![account_id, generation, target_set_hmac_sha256],
        |row| row.get(0),
    )?;
    if complete != 1 {
        anyhow::bail!("workflow cleanup is not exactly complete")
    }
    Ok(())
}

pub(crate) fn require_jobs_workflow_cleanup_complete_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    generation: i64,
    target_set_hmac_sha256: &str,
) -> Result<()> {
    let complete: bool = tx
        .query_one(
            "SELECT EXISTS (
               SELECT 1 FROM jobs_workflow_cleanup_generations generation
                WHERE generation.account_id = $1 AND generation.generation = $2
                  AND generation.target_set_hmac_sha256 = $3
                  AND generation.state = 'complete' AND generation.legacy_reconciled
                  AND generation.legacy_unresolved_count = 0
                  AND generation.target_count = (
                    SELECT COUNT(*)::bigint FROM jobs_workflow_cleanup_targets target
                     WHERE target.account_id = $1 AND target.generation = $2
                       AND target.target_set_hmac_sha256 = $3
                       AND target.target_state = 'absence_proved'
                  )
             )",
            &[&account_id, &generation, &target_set_hmac_sha256],
        )?
        .get(0);
    if !complete {
        anyhow::bail!("workflow cleanup is not exactly complete")
    }
    Ok(())
}

pub fn verify_jobs_workflow_cleanup_complete(
    pool: &DbPool,
    account_id: &str,
    generation: i64,
    target_set_hmac_sha256: &str,
) -> Result<bool> {
    if !workflow_command_identifier(account_id, 128)
        || !(1..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&generation)
        || target_set_hmac_sha256.len() != 64
    {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let result = require_jobs_workflow_cleanup_complete_sqlite_tx(
                &transaction,
                account_id,
                generation,
                target_set_hmac_sha256,
            )
            .is_ok();
            transaction.commit()?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let result = require_jobs_workflow_cleanup_complete_postgres_tx(
                &mut transaction,
                account_id,
                generation,
                target_set_hmac_sha256,
            )
            .is_ok();
            transaction.commit()?;
            Ok(result)
        }
    })
}

pub fn claim_jobs_workflow_command(
    pool: &DbPool,
    owner_id: &str,
    now_ms: i64,
    lease_ms: i64,
) -> Result<Option<JobsWorkflowCommandLease>> {
    claim_jobs_workflow_command_inner(pool, owner_id, now_ms, lease_ms, false)
}

/// Claims only commands that have already crossed request-start.
///
/// This path is deliberately narrower than ordinary dispatch so durable
/// Describe/Update-result reconciliation can continue while new-effect
/// dispatch is disabled. It never claims an unstarted command; historical v2
/// rows remain eligible only for a dedicated lookup-only gateway endpoint.
pub fn claim_jobs_workflow_command_reconciliation(
    pool: &DbPool,
    owner_id: &str,
    now_ms: i64,
    lease_ms: i64,
) -> Result<Option<JobsWorkflowCommandLease>> {
    claim_jobs_workflow_command_inner(pool, owner_id, now_ms, lease_ms, true)
}

fn claim_jobs_workflow_command_inner(
    pool: &DbPool,
    owner_id: &str,
    now_ms: i64,
    lease_ms: i64,
    reconciliation_only: bool,
) -> Result<Option<JobsWorkflowCommandLease>> {
    validate_workflow_command_lease_input(owner_id, now_ms, lease_ms)?;
    let lease_expires_at_ms = now_ms + lease_ms;
    let lease_token = workflow_command_random_lease_token();
    let lease_token_sha256 = workflow_command_lease_token_sha256(&lease_token);
    let attempt_id = new_jobs_workflow_attempt_id();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(expired) = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                           FROM jobs_workflow_commands
                          WHERE state IN ('claimed', 'delivering')
                            AND lease_expires_at_ms <= ?1
                          ORDER BY lease_expires_at_ms, id LIMIT 1"
                    ),
                    params![now_ms],
                    sqlite_workflow_command_row,
                )
                .optional()?
            {
                let request_started = transaction.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_workflow_command_attempt_events
                         WHERE attempt_id = ?1 AND event_kind = 'request_started'
                     )",
                    params![expired.active_attempt_id],
                    |row| row.get::<_, bool>(0),
                )?;
                if request_started {
                    transaction.execute(
                        "INSERT INTO jobs_workflow_command_attempt_events (
                            id, account_id, command_id, attempt_id, fence, event_phase,
                            event_kind, reason_code, temporal_run_id, recorded_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'terminal', 'delivery_unknown',
                            'lease_expired', NULL, ?6)
                         ON CONFLICT(attempt_id, event_phase) DO NOTHING",
                        params![
                            new_jobs_workflow_attempt_event_id(),
                            expired.account_id,
                            expired.id,
                            expired.active_attempt_id,
                            expired.fence,
                            now_ms,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE jobs_workflow_commands
                            SET state = 'delivery_unknown', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                first_ambiguous_at_ms = COALESCE(first_ambiguous_at_ms, ?1),
                                next_attempt_at_ms = ?1, last_outcome_code = 'lease_expired',
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE id = ?2 AND account_id = ?3
                            AND state IN ('claimed', 'delivering') AND fence = ?4",
                        params![now_ms, expired.id, expired.account_id, expired.fence],
                    )?;
                } else {
                    transaction.execute(
                        "INSERT INTO jobs_workflow_command_attempt_events (
                            id, account_id, command_id, attempt_id, fence, event_phase,
                            event_kind, reason_code, temporal_run_id, recorded_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'terminal',
                            'lease_expired_before_start', 'lease_expired', NULL, ?6)
                         ON CONFLICT(attempt_id, event_phase) DO NOTHING",
                        params![
                            new_jobs_workflow_attempt_event_id(),
                            expired.account_id,
                            expired.id,
                            expired.active_attempt_id,
                            expired.fence,
                            now_ms,
                        ],
                    )?;
                    transaction.execute(
                        "UPDATE jobs_workflow_commands
                            SET state = 'pending', lease_owner = NULL,
                                lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                                next_attempt_at_ms = ?1, last_outcome_code = 'lease_expired',
                                updated_at_ms = MAX(?1, updated_at_ms + 1)
                          WHERE id = ?2 AND account_id = ?3
                            AND state = 'claimed' AND fence = ?4",
                        params![now_ms, expired.id, expired.account_id, expired.fence],
                    )?;
                }
            }
            let candidate_state = if reconciliation_only {
                "command.state IN ('pending', 'delivery_unknown')
                   AND command.first_request_started_at_ms IS NOT NULL
                   AND EXISTS (
                     SELECT 1 FROM jobs_workflow_command_attempt_events started
                      WHERE started.account_id = command.account_id
                        AND started.command_id = command.id
                        AND started.event_kind = 'request_started'
                   )"
            } else {
                "command.state IN ('pending', 'delivery_unknown')
                   AND command.managed_cloud_authority_required = 1
                   AND command.first_request_started_at_ms IS NULL"
            };
            let candidate = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                           FROM jobs_workflow_commands command
                          WHERE {candidate_state}
                            AND command.next_attempt_at_ms <= ?1
                            AND command.fence < ?2
                            AND NOT EXISTS (
                              SELECT 1 FROM account_deletion_intents deletion
                               WHERE deletion.account_id = command.account_id
                            )
                            AND NOT EXISTS (
                              SELECT 1 FROM jobs_workflow_cleanup_generations cleanup
                               WHERE cleanup.account_id = command.account_id
                            )
                          ORDER BY command.next_attempt_at_ms, command.created_at_ms, command.id
                          LIMIT 1"
                    ),
                    params![now_ms, WORKFLOW_COMMAND_SAFE_INTEGER_MAX],
                    sqlite_workflow_command_row,
                )
                .optional()?;
            let Some(candidate) = candidate else {
                transaction.commit()?;
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &transaction,
                &candidate.account_id,
            )?;
            let fence = candidate
                .fence
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("workflow command fence overflow"))?;
            transaction.execute(
                "INSERT INTO jobs_workflow_command_attempts (
                    id, account_id, command_id, attempt_no, fence, lease_owner,
                    lease_token_sha256, request_id, payload_hmac_sha256,
                    claimed_at_ms, lease_expires_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    attempt_id,
                    candidate.account_id,
                    candidate.id,
                    fence,
                    owner_id,
                    lease_token_sha256,
                    candidate.request_id,
                    candidate.payload_hmac_sha256,
                    now_ms,
                    lease_expires_at_ms,
                ],
            )?;
            let changed = transaction.execute(
                "UPDATE jobs_workflow_commands
                    SET state = 'claimed', attempt_count = ?1, fence = ?1,
                        lease_owner = ?2, lease_token_sha256 = ?3,
                        lease_expires_at_ms = ?4, active_attempt_id = ?5,
                        next_attempt_at_ms = NULL, updated_at_ms = MAX(?6, updated_at_ms + 1)
                  WHERE id = ?7 AND account_id = ?8
                    AND state IN ('pending', 'delivery_unknown') AND fence = ?9",
                params![
                    fence,
                    owner_id,
                    lease_token_sha256,
                    lease_expires_at_ms,
                    attempt_id,
                    now_ms,
                    candidate.id,
                    candidate.account_id,
                    candidate.fence,
                ],
            )?;
            if changed != 1 {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            let stored = transaction.query_row(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
                ),
                params![candidate.account_id, candidate.id],
                sqlite_workflow_command_row,
            )?;
            transaction.commit()?;
            Ok(Some(JobsWorkflowCommandLease {
                command: workflow_command_from_stored(stored)?,
                attempt_id,
                lease_owner: owner_id.to_string(),
                lease_token,
                fence,
                lease_expires_at_ms,
            }))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let recovered_expired = if let Some(expired) = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands
                      WHERE state IN ('claimed', 'delivering')
                        AND lease_expires_at_ms <= $1
                      ORDER BY lease_expires_at_ms, id
                      FOR UPDATE SKIP LOCKED LIMIT 1"
                ),
                &[&now_ms],
            )? {
                let expired = postgres_workflow_command_row(&expired);
                let request_started: bool = transaction
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM jobs_workflow_command_attempt_events
                             WHERE attempt_id = $1 AND event_kind = 'request_started'
                         )",
                        &[&expired.active_attempt_id],
                    )?
                    .get(0);
                let (event_kind, next_state) = if request_started {
                    ("delivery_unknown", "delivery_unknown")
                } else {
                    ("lease_expired_before_start", "pending")
                };
                transaction.execute(
                    "INSERT INTO jobs_workflow_command_attempt_events (
                        id, account_id, command_id, attempt_id, fence, event_phase,
                        event_kind, reason_code, temporal_run_id, recorded_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, 'terminal', $6,
                        'lease_expired', NULL, $7)
                     ON CONFLICT(attempt_id, event_phase) DO NOTHING",
                    &[
                        &new_jobs_workflow_attempt_event_id(),
                        &expired.account_id,
                        &expired.id,
                        &expired.active_attempt_id,
                        &expired.fence,
                        &event_kind,
                        &now_ms,
                    ],
                )?;
                transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = $1, lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL,
                            first_ambiguous_at_ms = CASE WHEN $2 THEN
                              COALESCE(first_ambiguous_at_ms, $3) ELSE first_ambiguous_at_ms END,
                            next_attempt_at_ms = $3, last_outcome_code = 'lease_expired',
                            updated_at_ms = GREATEST($3, updated_at_ms + 1)
                      WHERE id = $4 AND account_id = $5
                        AND state IN ('claimed', 'delivering') AND fence = $6",
                    &[
                        &next_state,
                        &request_started,
                        &now_ms,
                        &expired.id,
                        &expired.account_id,
                        &expired.fence,
                    ],
                )?;
                true
            } else {
                false
            };
            if recovered_expired {
                transaction.commit()?;
                transaction = connection.transaction()?;
            }
            let candidate_state = if reconciliation_only {
                "command.state IN ('pending', 'delivery_unknown')
                   AND command.first_request_started_at_ms IS NOT NULL
                   AND EXISTS (
                     SELECT 1 FROM jobs_workflow_command_attempt_events started
                      WHERE started.account_id = command.account_id
                        AND started.command_id = command.id
                        AND started.event_kind = 'request_started'
                   )"
            } else {
                "command.state IN ('pending', 'delivery_unknown')
                   AND command.managed_cloud_authority_required
                   AND command.first_request_started_at_ms IS NULL"
            };
            let candidate_identity = transaction.query_opt(
                &format!(
                    "SELECT command.account_id, command.id AS command_id
                   FROM jobs_workflow_commands command
                   JOIN accounts account_row ON account_row.id = command.account_id
                  WHERE {candidate_state}
                    AND command.next_attempt_at_ms <= $1 AND command.fence < $2
                    AND NOT EXISTS (
                      SELECT 1 FROM account_deletion_intents deletion
                       WHERE deletion.account_id = command.account_id
                    )
                    AND NOT EXISTS (
                      SELECT 1 FROM jobs_workflow_cleanup_generations cleanup
                       WHERE cleanup.account_id = command.account_id
                    )
                  ORDER BY command.next_attempt_at_ms, command.created_at_ms, command.id
                  FOR UPDATE OF account_row SKIP LOCKED LIMIT 1"
                ),
                &[&now_ms, &WORKFLOW_COMMAND_SAFE_INTEGER_MAX],
            )?;
            let Some(candidate_identity) = candidate_identity else {
                transaction.commit()?;
                return Ok(None);
            };
            let candidate_account_id: String = candidate_identity.get("account_id");
            let candidate_command_id: String = candidate_identity.get("command_id");
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut transaction,
                &candidate_account_id,
            )?;
            require_no_workflow_cleanup_postgres_tx(&mut transaction, &candidate_account_id)?;
            let candidate = transaction.query_opt(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands command
                      WHERE command.account_id = $1 AND command.id = $2
                        AND {candidate_state}
                        AND command.next_attempt_at_ms <= $3 AND command.fence < $4
                        AND NOT EXISTS (
                          SELECT 1 FROM account_deletion_intents deletion
                           WHERE deletion.account_id = command.account_id
                        )
                        AND NOT EXISTS (
                          SELECT 1 FROM jobs_workflow_cleanup_generations cleanup
                           WHERE cleanup.account_id = command.account_id
                        )
                      FOR UPDATE SKIP LOCKED"
                ),
                &[
                    &candidate_account_id,
                    &candidate_command_id,
                    &now_ms,
                    &WORKFLOW_COMMAND_SAFE_INTEGER_MAX,
                ],
            )?;
            let Some(candidate) = candidate else {
                transaction.commit()?;
                return Ok(None);
            };
            let candidate = postgres_workflow_command_row(&candidate);
            let fence = candidate
                .fence
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("workflow command fence overflow"))?;
            transaction.execute(
                "INSERT INTO jobs_workflow_command_attempts (
                    id, account_id, command_id, attempt_no, fence, lease_owner,
                    lease_token_sha256, request_id, payload_hmac_sha256,
                    claimed_at_ms, lease_expires_at_ms
                 ) VALUES ($1, $2, $3, $4, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &attempt_id,
                    &candidate.account_id,
                    &candidate.id,
                    &fence,
                    &owner_id,
                    &lease_token_sha256,
                    &candidate.request_id,
                    &candidate.payload_hmac_sha256,
                    &now_ms,
                    &lease_expires_at_ms,
                ],
            )?;
            let changed = transaction.execute(
                "UPDATE jobs_workflow_commands
                    SET state = 'claimed', attempt_count = $1, fence = $1,
                        lease_owner = $2, lease_token_sha256 = $3,
                        lease_expires_at_ms = $4, active_attempt_id = $5,
                        next_attempt_at_ms = NULL,
                        updated_at_ms = GREATEST($6, updated_at_ms + 1)
                  WHERE id = $7 AND account_id = $8
                    AND state IN ('pending', 'delivery_unknown') AND fence = $9",
                &[
                    &fence,
                    &owner_id,
                    &lease_token_sha256,
                    &lease_expires_at_ms,
                    &attempt_id,
                    &now_ms,
                    &candidate.id,
                    &candidate.account_id,
                    &candidate.fence,
                ],
            )?;
            if changed != 1 {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            let stored = transaction.query_one(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = $1 AND id = $2"
                ),
                &[&candidate.account_id, &candidate.id],
            )?;
            transaction.commit()?;
            Ok(Some(JobsWorkflowCommandLease {
                command: workflow_command_from_stored(postgres_workflow_command_row(&stored))?,
                attempt_id,
                lease_owner: owner_id.to_string(),
                lease_token,
                fence,
                lease_expires_at_ms,
            }))
        }
    })
}

pub fn mark_jobs_workflow_command_request_started(
    pool: &DbPool,
    lease: &JobsWorkflowCommandLease,
    now_ms: i64,
) -> Result<JobsWorkflowCommand> {
    mark_jobs_workflow_command_request_started_with_managed_cloud(pool, lease, now_ms)
        .map(|(command, _)| command)
}

pub fn mark_jobs_workflow_command_request_started_with_managed_cloud(
    pool: &DbPool,
    lease: &JobsWorkflowCommandLease,
    now_ms: i64,
) -> Result<(
    JobsWorkflowCommand,
    Option<ManagedCloudRequestStartAuthority>,
)> {
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    let managed_cloud_scope =
        crate::jobs_managed_cloud_runtime::managed_cloud_scope_for_admission();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let stored = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                           FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
                    ),
                    params![lease.command.account_id, lease.command.id],
                    sqlite_workflow_command_row,
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let claimed = validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Claimed,
                now_ms,
            )
            .is_ok();
            let delivering = validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Delivering,
                now_ms,
            )
            .is_ok();
            if !claimed && !delivering {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &transaction,
                &stored.account_id,
            )?;
            require_no_workflow_cleanup_sqlite_tx(&transaction, &stored.account_id)?;
            let managed_cloud = resolve_managed_cloud_request_start_sqlite_tx(
                &transaction,
                lease,
                managed_cloud_scope.as_ref(),
            )?;
            if delivering {
                if managed_cloud
                    .as_ref()
                    .is_none_or(|authority| !authority.attempt_replayed)
                {
                    return Err(JobsWorkflowCommandError::StaleLease.into());
                }
                transaction.commit()?;
                return Ok((workflow_command_from_stored(stored)?, managed_cloud));
            }
            if managed_cloud
                .as_ref()
                .is_some_and(|authority| authority.attempt_replayed)
            {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            transaction.execute(
                "INSERT INTO jobs_workflow_command_attempt_events (
                    id, account_id, command_id, attempt_id, fence, event_phase,
                    event_kind, reason_code, temporal_run_id, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'request_started',
                    'request_started', NULL, NULL, ?6)",
                params![
                    new_jobs_workflow_attempt_event_id(),
                    stored.account_id,
                    stored.id,
                    lease.attempt_id,
                    lease.fence,
                    now_ms,
                ],
            )?;
            let changed = transaction.execute(
                "UPDATE jobs_workflow_commands
                    SET state = 'delivering',
                        first_request_started_at_ms = COALESCE(first_request_started_at_ms, ?1),
                        updated_at_ms = MAX(?1, updated_at_ms + 1)
                  WHERE id = ?2 AND account_id = ?3 AND state = 'claimed'
                    AND active_attempt_id = ?4 AND fence = ?5
                    AND lease_owner = ?6 AND lease_token_sha256 = ?7
                    AND lease_expires_at_ms = ?8",
                params![
                    now_ms,
                    stored.id,
                    stored.account_id,
                    lease.attempt_id,
                    lease.fence,
                    lease.lease_owner,
                    workflow_command_lease_token_sha256(&lease.lease_token),
                    lease.lease_expires_at_ms,
                ],
            )?;
            if changed != 1 {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            let updated = transaction.query_row(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
                ),
                params![stored.account_id, stored.id],
                sqlite_workflow_command_row,
            )?;
            transaction.commit()?;
            Ok((workflow_command_from_stored(updated)?, managed_cloud))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let mut preflight = managed_cloud_request_start_preflight_postgres_tx(
                &mut transaction,
                &lease.command.id,
            )?;
            if preflight == ManagedCloudRequestStartPreflight::FreshEffect {
                let scope = managed_cloud_scope
                    .as_ref()
                    .ok_or(ManagedCloudRegistryError::Unavailable)?;
                lock_managed_cloud_workflow_admission_postgres_tx(&mut transaction, scope)?;
                // A concurrent first request-start may have committed while
                // this transaction waited on the release lock. Reclassify it
                // before taking account/effect locks so recovery reuses the
                // immutable original authority without consulting current
                // flags, readiness, or activation state.
                preflight = managed_cloud_request_start_preflight_postgres_tx(
                    &mut transaction,
                    &lease.command.id,
                )?;
            }
            lock_discovery_account_shared_postgres(&mut transaction, &lease.command.account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut transaction,
                &lease.command.account_id,
            )?;
            require_no_workflow_cleanup_postgres_tx(&mut transaction, &lease.command.account_id)?;
            let resolver_scope = (preflight == ManagedCloudRequestStartPreflight::FreshEffect)
                .then_some(managed_cloud_scope.as_ref())
                .flatten();
            let managed_cloud = resolve_managed_cloud_request_start_postgres_tx(
                &mut transaction,
                lease,
                resolver_scope,
            )?;
            let row = transaction
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                           FROM jobs_workflow_commands
                          WHERE account_id = $1 AND id = $2 FOR UPDATE"
                    ),
                    &[&lease.command.account_id, &lease.command.id],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let stored = postgres_workflow_command_row(&row);
            let claimed = validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Claimed,
                now_ms,
            )
            .is_ok();
            let delivering = validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Delivering,
                now_ms,
            )
            .is_ok();
            if !claimed && !delivering {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            if delivering {
                if managed_cloud
                    .as_ref()
                    .is_none_or(|authority| !authority.attempt_replayed)
                {
                    return Err(JobsWorkflowCommandError::StaleLease.into());
                }
                transaction.commit()?;
                return Ok((workflow_command_from_stored(stored)?, managed_cloud));
            }
            if managed_cloud
                .as_ref()
                .is_some_and(|authority| authority.attempt_replayed)
            {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            transaction.execute(
                "INSERT INTO jobs_workflow_command_attempt_events (
                    id, account_id, command_id, attempt_id, fence, event_phase,
                    event_kind, reason_code, temporal_run_id, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'request_started',
                    'request_started', NULL, NULL, $6)",
                &[
                    &new_jobs_workflow_attempt_event_id(),
                    &stored.account_id,
                    &stored.id,
                    &lease.attempt_id,
                    &lease.fence,
                    &now_ms,
                ],
            )?;
            let token_sha256 = workflow_command_lease_token_sha256(&lease.lease_token);
            let changed = transaction.execute(
                "UPDATE jobs_workflow_commands
                    SET state = 'delivering',
                        first_request_started_at_ms = COALESCE(first_request_started_at_ms, $1),
                        updated_at_ms = GREATEST($1, updated_at_ms + 1)
                  WHERE id = $2 AND account_id = $3 AND state = 'claimed'
                    AND active_attempt_id = $4 AND fence = $5
                    AND lease_owner = $6 AND lease_token_sha256 = $7
                    AND lease_expires_at_ms = $8",
                &[
                    &now_ms,
                    &stored.id,
                    &stored.account_id,
                    &lease.attempt_id,
                    &lease.fence,
                    &lease.lease_owner,
                    &token_sha256,
                    &lease.lease_expires_at_ms,
                ],
            )?;
            if changed != 1 {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            let updated = transaction.query_one(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = $1 AND id = $2"
                ),
                &[&stored.account_id, &stored.id],
            )?;
            transaction.commit()?;
            Ok((
                workflow_command_from_stored(postgres_workflow_command_row(&updated))?,
                managed_cloud,
            ))
        }
    })
}

pub fn complete_jobs_workflow_command(
    pool: &DbPool,
    lease: &JobsWorkflowCommandLease,
    completion: JobsWorkflowCommandCompletion,
    now_ms: i64,
    retry_at_ms: Option<i64>,
) -> Result<JobsWorkflowCommand> {
    if !(0..=WORKFLOW_COMMAND_SAFE_INTEGER_MAX).contains(&now_ms) {
        return Err(JobsWorkflowCommandError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let stored = transaction
                .query_row(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                           FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
                    ),
                    params![lease.command.account_id, lease.command.id],
                    sqlite_workflow_command_row,
                )
                .optional()?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Delivering,
                now_ms,
            )?;
            let next_attempt_at_ms =
                validate_workflow_command_completion(&stored, &completion, retry_at_ms, now_ms)?;
            let (event_kind, reason_code, temporal_run_id) = workflow_command_event(&completion);
            transaction.execute(
                "INSERT INTO jobs_workflow_command_attempt_events (
                    id, account_id, command_id, attempt_id, fence, event_phase,
                    event_kind, reason_code, temporal_run_id, recorded_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'terminal', ?6, ?7, ?8, ?9)",
                params![
                    new_jobs_workflow_attempt_event_id(),
                    stored.account_id,
                    stored.id,
                    lease.attempt_id,
                    lease.fence,
                    event_kind,
                    reason_code,
                    temporal_run_id,
                    now_ms,
                ],
            )?;
            let token_sha256 = workflow_command_lease_token_sha256(&lease.lease_token);
            let changed = match &completion {
                JobsWorkflowCommandCompletion::Accepted(receipt) => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'accepted', lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL, next_attempt_at_ms = NULL,
                            last_outcome_code = ?1, temporal_run_id = ?2,
                            accepted_at_ms = ?3, updated_at_ms = MAX(?3, updated_at_ms + 1)
                      WHERE id = ?4 AND account_id = ?5 AND state = 'delivering'
                        AND active_attempt_id = ?6 AND fence = ?7
                        AND lease_owner = ?8 AND lease_token_sha256 = ?9",
                    params![
                        receipt.outcome.event_kind(),
                        receipt.temporal_run_id,
                        now_ms,
                        stored.id,
                        stored.account_id,
                        lease.attempt_id,
                        lease.fence,
                        lease.lease_owner,
                        token_sha256,
                    ],
                )?,
                JobsWorkflowCommandCompletion::DeliveryUnknown(reason) => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'delivery_unknown', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            first_ambiguous_at_ms = COALESCE(first_ambiguous_at_ms, ?1),
                            next_attempt_at_ms = ?2, last_outcome_code = ?3,
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE id = ?4 AND account_id = ?5 AND state = 'delivering'
                        AND active_attempt_id = ?6 AND fence = ?7
                        AND lease_owner = ?8 AND lease_token_sha256 = ?9",
                    params![
                        now_ms,
                        next_attempt_at_ms,
                        reason.as_str(),
                        stored.id,
                        stored.account_id,
                        lease.attempt_id,
                        lease.fence,
                        lease.lease_owner,
                        token_sha256,
                    ],
                )?,
                JobsWorkflowCommandCompletion::IdentityConflict => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'identity_conflict', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = NULL, last_outcome_code = 'identity_conflict',
                            updated_at_ms = MAX(?1, updated_at_ms + 1)
                      WHERE id = ?2 AND account_id = ?3 AND state = 'delivering'
                        AND active_attempt_id = ?4 AND fence = ?5
                        AND lease_owner = ?6 AND lease_token_sha256 = ?7",
                    params![
                        now_ms,
                        stored.id,
                        stored.account_id,
                        lease.attempt_id,
                        lease.fence,
                        lease.lease_owner,
                        token_sha256,
                    ],
                )?,
                JobsWorkflowCommandCompletion::Rejected(reason) => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'rejected', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = NULL, last_outcome_code = ?1,
                            updated_at_ms = MAX(?2, updated_at_ms + 1)
                      WHERE id = ?3 AND account_id = ?4 AND state = 'delivering'
                        AND active_attempt_id = ?5 AND fence = ?6
                        AND lease_owner = ?7 AND lease_token_sha256 = ?8",
                    params![
                        reason.as_str(),
                        now_ms,
                        stored.id,
                        stored.account_id,
                        lease.attempt_id,
                        lease.fence,
                        lease.lease_owner,
                        token_sha256,
                    ],
                )?,
            };
            if changed != 1 {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            match &completion {
                JobsWorkflowCommandCompletion::Accepted(receipt) => {
                    bind_workflow_execution_acceptance_sqlite_tx(
                        &transaction,
                        &stored,
                        receipt,
                        now_ms,
                    )?;
                }
                JobsWorkflowCommandCompletion::IdentityConflict => {
                    terminalize_workflow_rows_sqlite_tx(
                        &transaction,
                        &stored,
                        "side_effect_unknown",
                        now_ms,
                    )?;
                }
                JobsWorkflowCommandCompletion::Rejected(_) => {
                    terminalize_workflow_rows_sqlite_tx(&transaction, &stored, "failed", now_ms)?;
                }
                JobsWorkflowCommandCompletion::DeliveryUnknown(_) => {}
            }
            let updated = transaction.query_row(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = ?1 AND id = ?2"
                ),
                params![stored.account_id, stored.id],
                sqlite_workflow_command_row,
            )?;
            transaction.commit()?;
            workflow_command_from_stored(updated)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection.transaction()?;
            let row = transaction
                .query_opt(
                    &format!(
                        "SELECT {WORKFLOW_COMMAND_SELECT}
                           FROM jobs_workflow_commands
                          WHERE account_id = $1 AND id = $2 FOR UPDATE"
                    ),
                    &[&lease.command.account_id, &lease.command.id],
                )?
                .ok_or(JobsWorkflowCommandError::NotFound)?;
            let stored = postgres_workflow_command_row(&row);
            validate_workflow_command_lease(
                &stored,
                lease,
                JobsWorkflowCommandState::Delivering,
                now_ms,
            )?;
            let next_attempt_at_ms =
                validate_workflow_command_completion(&stored, &completion, retry_at_ms, now_ms)?;
            let (event_kind, reason_code, temporal_run_id) = workflow_command_event(&completion);
            transaction.execute(
                "INSERT INTO jobs_workflow_command_attempt_events (
                    id, account_id, command_id, attempt_id, fence, event_phase,
                    event_kind, reason_code, temporal_run_id, recorded_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'terminal', $6, $7, $8, $9)",
                &[
                    &new_jobs_workflow_attempt_event_id(),
                    &stored.account_id,
                    &stored.id,
                    &lease.attempt_id,
                    &lease.fence,
                    &event_kind,
                    &reason_code,
                    &temporal_run_id,
                    &now_ms,
                ],
            )?;
            let token_sha256 = workflow_command_lease_token_sha256(&lease.lease_token);
            let changed = match &completion {
                JobsWorkflowCommandCompletion::Accepted(receipt) => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'accepted', lease_owner = NULL, lease_token_sha256 = NULL,
                            lease_expires_at_ms = NULL, next_attempt_at_ms = NULL,
                            last_outcome_code = $1, temporal_run_id = $2,
                            accepted_at_ms = $3, updated_at_ms = GREATEST($3, updated_at_ms + 1)
                      WHERE id = $4 AND account_id = $5 AND state = 'delivering'
                        AND active_attempt_id = $6 AND fence = $7
                        AND lease_owner = $8 AND lease_token_sha256 = $9",
                    &[
                        &receipt.outcome.event_kind(),
                        &receipt.temporal_run_id,
                        &now_ms,
                        &stored.id,
                        &stored.account_id,
                        &lease.attempt_id,
                        &lease.fence,
                        &lease.lease_owner,
                        &token_sha256,
                    ],
                )?,
                JobsWorkflowCommandCompletion::DeliveryUnknown(reason) => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'delivery_unknown', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            first_ambiguous_at_ms = COALESCE(first_ambiguous_at_ms, $1),
                            next_attempt_at_ms = $2, last_outcome_code = $3,
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE id = $4 AND account_id = $5 AND state = 'delivering'
                        AND active_attempt_id = $6 AND fence = $7
                        AND lease_owner = $8 AND lease_token_sha256 = $9",
                    &[
                        &now_ms,
                        &next_attempt_at_ms,
                        &reason.as_str(),
                        &stored.id,
                        &stored.account_id,
                        &lease.attempt_id,
                        &lease.fence,
                        &lease.lease_owner,
                        &token_sha256,
                    ],
                )?,
                JobsWorkflowCommandCompletion::IdentityConflict => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'identity_conflict', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = NULL, last_outcome_code = 'identity_conflict',
                            updated_at_ms = GREATEST($1, updated_at_ms + 1)
                      WHERE id = $2 AND account_id = $3 AND state = 'delivering'
                        AND active_attempt_id = $4 AND fence = $5
                        AND lease_owner = $6 AND lease_token_sha256 = $7",
                    &[
                        &now_ms,
                        &stored.id,
                        &stored.account_id,
                        &lease.attempt_id,
                        &lease.fence,
                        &lease.lease_owner,
                        &token_sha256,
                    ],
                )?,
                JobsWorkflowCommandCompletion::Rejected(reason) => transaction.execute(
                    "UPDATE jobs_workflow_commands
                        SET state = 'rejected', lease_owner = NULL,
                            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                            next_attempt_at_ms = NULL, last_outcome_code = $1,
                            updated_at_ms = GREATEST($2, updated_at_ms + 1)
                      WHERE id = $3 AND account_id = $4 AND state = 'delivering'
                        AND active_attempt_id = $5 AND fence = $6
                        AND lease_owner = $7 AND lease_token_sha256 = $8",
                    &[
                        &reason.as_str(),
                        &now_ms,
                        &stored.id,
                        &stored.account_id,
                        &lease.attempt_id,
                        &lease.fence,
                        &lease.lease_owner,
                        &token_sha256,
                    ],
                )?,
            };
            if changed != 1 {
                return Err(JobsWorkflowCommandError::StaleLease.into());
            }
            match &completion {
                JobsWorkflowCommandCompletion::Accepted(receipt) => {
                    bind_workflow_execution_acceptance_postgres_tx(
                        &mut transaction,
                        &stored,
                        receipt,
                        now_ms,
                    )?;
                }
                JobsWorkflowCommandCompletion::IdentityConflict => {
                    terminalize_workflow_rows_postgres_tx(
                        &mut transaction,
                        &stored,
                        "side_effect_unknown",
                        now_ms,
                    )?;
                }
                JobsWorkflowCommandCompletion::Rejected(_) => {
                    terminalize_workflow_rows_postgres_tx(
                        &mut transaction,
                        &stored,
                        "failed",
                        now_ms,
                    )?;
                }
                JobsWorkflowCommandCompletion::DeliveryUnknown(_) => {}
            }
            let updated = transaction.query_one(
                &format!(
                    "SELECT {WORKFLOW_COMMAND_SELECT}
                       FROM jobs_workflow_commands WHERE account_id = $1 AND id = $2"
                ),
                &[&stored.account_id, &stored.id],
            )?;
            transaction.commit()?;
            workflow_command_from_stored(postgres_workflow_command_row(&updated))
        }
    })
}

#[cfg(test)]
mod workflow_command_tests {
    use super::*;

    fn function_source<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start = source.find(start).expect("function start must exist");
        let end = source[start..]
            .find(end)
            .map(|offset| start + offset)
            .expect("function end must exist");
        &source[start..end]
    }

    fn cleanup_lease() -> JobsWorkflowCleanupLease {
        JobsWorkflowCleanupLease {
            account_id: "account-workflow-test-0001".to_string(),
            workflow_id: "bluey-jobs-v2-workflow-test-0001".to_string(),
            start_command_id: "wfcommand-v2-test-0001".to_string(),
            start_request_id: "wfreq-v2-request-test-0001".to_string(),
            start_payload_hmac_sha256: "a".repeat(64),
            first_execution_run_id: Some("temporal-run-test-0001".to_string()),
            generation: 1,
            target_set_hmac_sha256: "b".repeat(64),
            cleanup_state: "cleanup_required".to_string(),
            fence: 7,
            cleanup_request_id: "wfcleanupreq-v2-test-0001".to_string(),
            lease_owner: "cleanup-worker-test".to_string(),
            lease_token: "cleanup-lease-token-test-0001".to_string(),
            lease_expires_at_ms: 10_000,
        }
    }

    fn absence_receipt(lease: &JobsWorkflowCleanupLease) -> Value {
        json!({
            "schemaVersion": 2,
            "provider": "temporal",
            "cleanupRequestId": lease.cleanup_request_id,
            "cleanupFence": lease.fence,
            "requestId": lease.start_request_id,
            "workflowId": lease.workflow_id,
            "payloadHmacSha256": lease.start_payload_hmac_sha256,
            "observationKind": "absence_proved",
            "status": "complete",
            "firstExecutionRunId": lease.first_execution_run_id,
            "observedAtMs": 5_000,
            "workflowType": "applicationWorkflowV2",
            "taskQueue": "bluey_jobs_workflow_test_queue",
            "memo": {
                "schemaVersion": 2,
                "requestId": lease.start_request_id,
                "workflowId": lease.workflow_id,
                "payloadDigest": lease.start_payload_hmac_sha256,
            },
            "outcome": "complete",
            "reason": "absence_proved",
            "enumerationComplete": true,
            "nextPageToken": null,
            "runs": [{
                "runId": lease.first_execution_run_id,
                "describe": "not_found",
                "history": "not_found",
                "visibility": "not_found",
            }],
        })
    }

    #[test]
    fn terminal_outcome_matrix_is_closed() {
        assert!(
            JobsWorkflowTerminalOutcome::Failed(JobsWorkflowTerminalReason::RunnerFailed).valid()
        );
        assert!(JobsWorkflowTerminalOutcome::Failed(
            JobsWorkflowTerminalReason::InterventionTimeout
        )
        .valid());
        assert!(
            JobsWorkflowTerminalOutcome::Failed(JobsWorkflowTerminalReason::InterventionLimit)
                .valid()
        );
        assert!(JobsWorkflowTerminalOutcome::SideEffectUnknown(
            JobsWorkflowTerminalReason::RunnerAmbiguous
        )
        .valid());
        assert!(
            !JobsWorkflowTerminalOutcome::Failed(JobsWorkflowTerminalReason::RunnerAmbiguous)
                .valid()
        );
        assert!(!JobsWorkflowTerminalOutcome::SideEffectUnknown(
            JobsWorkflowTerminalReason::RunnerFailed
        )
        .valid());
    }

    #[test]
    fn cloud_queue_and_request_start_never_downgrade_managed_authority() {
        let source = include_str!("workflow_commands.rs");
        let start = function_source(
            source,
            "pub fn stage_cloud_workflow_start(",
            "fn managed_cloud_binding_input(",
        );
        let resume = function_source(
            source,
            "pub fn stage_cloud_workflow_resume(",
            "fn workflow_command_random_lease_token(",
        );
        for admission in [start, resume] {
            assert!(admission.contains("bind_managed_cloud_workflow_sqlite_tx"));
            assert!(admission.contains("bind_managed_cloud_workflow_postgres_tx"));
            assert!(admission.contains("require_managed_cloud_workflow_binding_replay_sqlite_tx"));
            assert!(admission.contains("require_managed_cloud_workflow_binding_replay_postgres_tx"));
            let postgres = &admission[admission
                .find("DbPool::Postgres(_) =>")
                .expect("Postgres admission branch")..];
            let managed_cloud_prelock = postgres
                .find("lock_managed_cloud_workflow_admission_postgres_tx")
                .expect("managed-cloud prelock");
            let account_write_fence = postgres
                .find("require_active_account_write_fence_postgres_tx")
                .expect("account write fence");
            let discovery_lock = postgres
                .find("lock_discovery_account_shared_postgres")
                .expect("shared discovery-account lock");
            assert!(managed_cloud_prelock < discovery_lock);
            assert!(discovery_lock < account_write_fence);
            assert!(postgres[managed_cloud_prelock..account_write_fence]
                .contains("require_managed_cloud_workflow_binding_replay_postgres_tx"));
        }

        let request_start = function_source(
            source,
            "pub fn mark_jobs_workflow_command_request_started_with_managed_cloud(",
            "pub fn complete_jobs_workflow_command(",
        );
        for resolver in [
            "resolve_managed_cloud_request_start_sqlite_tx",
            "resolve_managed_cloud_request_start_postgres_tx",
        ] {
            assert!(request_start.contains(resolver));
        }
        assert!(request_start.contains("managed_cloud_scope.as_ref()"));
        let request_start_postgres = &request_start[request_start
            .find("DbPool::Postgres(_) =>")
            .expect("Postgres request-start branch")..];
        assert!(
            request_start_postgres
                .find("lock_managed_cloud_workflow_admission_postgres_tx")
                .expect("Postgres release prelock")
                < request_start_postgres
                    .find("lock_discovery_account_shared_postgres")
                    .expect("Postgres shared discovery-account lock")
        );
        assert!(
            request_start_postgres
                .find("lock_discovery_account_shared_postgres")
                .expect("Postgres shared discovery-account lock")
                < request_start_postgres
                    .find("require_active_account_write_fence_postgres_tx")
                    .expect("Postgres account fence")
        );
        assert!(
            request_start_postgres
                .find("require_active_account_write_fence_postgres_tx")
                .expect("Postgres account fence")
                < request_start_postgres
                    .find("resolve_managed_cloud_request_start_postgres_tx")
                    .expect("Postgres release resolver")
        );
    }

    #[test]
    fn effect_claim_is_managed_and_reconciliation_is_request_started_cleanup_exclusive() {
        let source = include_str!("workflow_commands.rs");
        let claim = function_source(
            source,
            "pub fn claim_jobs_workflow_command(",
            "pub fn mark_jobs_workflow_command_request_started(",
        );

        assert!(claim.contains("pub fn claim_jobs_workflow_command_reconciliation("));
        assert!(claim.contains("command.state IN ('pending', 'delivery_unknown')"));
        assert!(claim.contains("command.managed_cloud_authority_required = 1"));
        assert!(claim.contains("command.managed_cloud_authority_required\n"));
        assert!(claim.contains("command.first_request_started_at_ms IS NOT NULL"));
        assert!(claim.contains("command.first_request_started_at_ms IS NULL"));
        assert!(claim.contains("started.event_kind = 'request_started'"));
        assert!(claim.contains(
            "NOT EXISTS (\n                              SELECT 1 FROM account_deletion_intents"
        ));
        assert!(claim.contains("NOT EXISTS (\n                              SELECT 1 FROM jobs_workflow_cleanup_generations"));
    }

    #[test]
    fn runner_terminal_open_intervention_queries_have_dialect_parity() {
        for sql in [
            SQLITE_WORKFLOW_APPLICATION_AUTHORITY_SQL,
            POSTGRES_WORKFLOW_APPLICATION_AUTHORITY_SQL,
        ] {
            assert!(sql.contains("FROM jobs_applications"));
            assert!(sql.contains("account_id"));
            assert!(sql.contains("id"));
        }
        assert!(POSTGRES_WORKFLOW_APPLICATION_AUTHORITY_SQL.contains("FOR UPDATE"));

        for sql in [
            SQLITE_OPEN_APPLICATION_INTERVENTIONS_SQL,
            POSTGRES_OPEN_APPLICATION_INTERVENTIONS_SQL,
        ] {
            assert!(sql.contains("FROM jobs_interventions"));
            assert!(sql.contains("account_id"));
            assert!(sql.contains("application_id"));
            assert!(sql.contains("status = 'open'"));
        }
    }

    #[test]
    fn postgres_materialization_locks_account_before_exact_command() {
        let source = include_str!("workflow_commands.rs");
        let materialization = function_source(
            source,
            "pub fn get_materializable_jobs_workflow_command_by_request_id(",
            "pub fn prepare_jobs_workflow_intervention(",
        );
        let postgres = &materialization[materialization
            .find("DbPool::Postgres(_) =>")
            .expect("Postgres materialization branch must exist")..];

        let identity_lookup = postgres
            .find("SELECT command.account_id, command.id")
            .expect("nonlocking materialization identity lookup must exist");
        let account_fence = postgres
            .find("require_active_account_write_fence_postgres_tx(")
            .expect("materialization account fence must exist");
        let exact_command = postgres
            .find("WHERE command.account_id = $1 AND command.id = $2")
            .expect("exact materialization command recheck must exist");
        let exact_command_lock = postgres[exact_command..]
            .find("FOR SHARE")
            .map(|offset| exact_command + offset)
            .expect("exact materialization command must be share locked");

        assert!(identity_lookup < account_fence);
        assert!(account_fence < exact_command);
        assert!(exact_command < exact_command_lock);
        assert!(!postgres[identity_lookup..account_fence].contains("FOR SHARE"));
        assert!(!postgres[identity_lookup..account_fence].contains("FOR UPDATE"));
        for exact_recheck in [
            "command.request_id = $3",
            "command.first_request_started_at_ms IS NOT NULL",
            "event.event_kind = 'request_started'",
            "FROM jobs_workflow_cleanup_generations cleanup",
        ] {
            assert!(postgres[exact_command..exact_command_lock].contains(exact_recheck));
        }
    }

    #[test]
    fn postgres_claim_locks_candidate_account_before_exact_command() {
        let source = include_str!("workflow_commands.rs");
        let claim = function_source(
            source,
            "pub fn claim_jobs_workflow_command(",
            "pub fn mark_jobs_workflow_command_request_started(",
        );
        let postgres = &claim[claim
            .find("DbPool::Postgres(_) =>")
            .expect("Postgres claim branch must exist")..];

        let expired_command_lock = postgres
            .find("FOR UPDATE SKIP LOCKED LIMIT 1")
            .expect("expired command recovery lock must exist");
        let recovery_boundary = postgres
            .find("if recovered_expired {")
            .expect("expired recovery transaction boundary must exist");
        let fresh_transaction = postgres[recovery_boundary..]
            .find("transaction = connection.transaction()?;")
            .map(|offset| recovery_boundary + offset)
            .expect("candidate claim must start a fresh transaction after recovery");
        let account_candidate_lock = postgres
            .find("FOR UPDATE OF account_row SKIP LOCKED LIMIT 1")
            .expect("candidate claim must lock and skip locked accounts");
        let account_fence = postgres
            .find("require_active_account_write_fence_postgres_tx(")
            .expect("candidate account fence validation must exist");
        let exact_command = postgres
            .find("WHERE command.account_id = $1 AND command.id = $2")
            .expect("exact candidate command recheck must exist");
        let exact_command_lock = postgres[exact_command..]
            .find("FOR UPDATE SKIP LOCKED")
            .map(|offset| exact_command + offset)
            .expect("exact candidate command must be locked without waiting");

        assert!(expired_command_lock < recovery_boundary);
        assert!(recovery_boundary < fresh_transaction);
        assert!(fresh_transaction < account_candidate_lock);
        assert!(account_candidate_lock < account_fence);
        assert!(account_fence < exact_command);
        assert!(exact_command < exact_command_lock);
        assert!(postgres.contains("JOIN accounts account_row"));
    }

    #[test]
    fn postgres_request_start_locks_account_before_command_and_event() {
        let source = include_str!("workflow_commands.rs");
        let request_start = function_source(
            source,
            "pub fn mark_jobs_workflow_command_request_started(",
            "pub fn complete_jobs_workflow_command(",
        );
        let postgres = &request_start[request_start
            .find("DbPool::Postgres(_) =>")
            .expect("Postgres request-start branch must exist")..];

        let account_fence = postgres
            .find("require_active_account_write_fence_postgres_tx(")
            .expect("request-start account fence validation must exist");
        let cleanup_fence = postgres
            .find("require_no_workflow_cleanup_postgres_tx(")
            .expect("request-start cleanup fence validation must exist");
        let command_lock = postgres
            .find("WHERE account_id = $1 AND id = $2 FOR UPDATE")
            .expect("request-start exact command lock must exist");
        let durable_start_event = postgres
            .find("INSERT INTO jobs_workflow_command_attempt_events")
            .expect("durable request-start event must exist");

        assert!(account_fence < cleanup_fence);
        assert!(cleanup_fence < command_lock);
        assert!(command_lock < durable_start_event);
    }

    #[test]
    fn absence_receipt_requires_one_atomic_closed_not_found_proof() {
        let lease = cleanup_lease();
        let accepted = CompleteJobsWorkflowCleanupTarget {
            observation_kind: JobsWorkflowCleanupObservationKind::AbsenceProved,
            first_execution_run_id: lease.first_execution_run_id.clone(),
            evidence: absence_receipt(&lease),
            now_ms: 5_000,
        };
        validate_jobs_workflow_cleanup_evidence(&lease, &accepted).unwrap();

        let mut partial = accepted.clone();
        partial.evidence["runs"][0]["history"] = json!("found");
        assert!(validate_jobs_workflow_cleanup_evidence(&lease, &partial).is_err());

        let mut paginated = accepted;
        paginated.evidence["enumerationComplete"] = json!(false);
        paginated.evidence["nextPageToken"] = json!("more-results");
        assert!(validate_jobs_workflow_cleanup_evidence(&lease, &paginated).is_err());
    }
}
