//! Deterministic Phase 620A1 verifier compiled only for no-egress contract tests.
//!
//! Every accepted identifier is visibly synthetic. Every projection is simulated, and the only
//! effect audit this module can construct contains zero attempts.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;

pub const SIMULATOR_VERSION: u8 = 1;
pub const GRAMMAR_VERSION: &str = "business_messaging_command_grammar_v1";
pub const WHATSAPP_STATUS_EVIDENCE_REVISION: &str = "rev_test_whatsapp_status_schema_v1";
pub const WHATSAPP_GATEWAY_EVIDENCE_REVISION: &str = "rev_test_whatsapp_gateway_v1";
pub const WHATSAPP_CLOSE_EVIDENCE_REVISION: &str = "rev_test_whatsapp_close_schema_v1";
pub const APPLE_GATEWAY_EVIDENCE_REVISION: &str = "rev_test_apple_gateway_v1";
pub const APPLE_CLOSE_EVIDENCE_REVISION: &str = "rev_test_apple_close_schema_v1";
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessProvider {
    WhatsappBusinessPlatform,
    AppleMessagesForBusiness,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPolicyMode {
    WhatsappCustomerServiceWindow,
    AppleActiveConversation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptedOutcome {
    SimulatedNoEffect,
    ProviderAccepted,
    WhatsappDelivered,
    WhatsappRead,
    FailedPreRequest,
    TimeoutAfterRequestStart,
    ProviderClosed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatorReadSet {
    pub connection_revision: String,
    pub consent_revision: String,
    pub opt_out_parser_revision: String,
    pub command_parser_revision: String,
    pub locale_table_revision: String,
    pub provider_policy_revision: String,
    pub provider_eligibility_revision: String,
    pub jobs_workspace_revision: String,
    pub career_track_revision: String,
    pub original_source_revision: String,
    pub job_integrity_revision: String,
    pub adapter_release_revision: String,
    pub kill_switch_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatorAuthority {
    pub connection_active: bool,
    pub consent_active: bool,
    pub provider_eligible: bool,
    pub track_eligible: bool,
    pub source_verified: bool,
    pub integrity_verified: bool,
    pub suppression_active: bool,
    pub kill_switch_active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatorProviderPolicy {
    pub mode: ProviderPolicyMode,
    pub status_evidence_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatorInput {
    pub version: u8,
    pub provider: BusinessProvider,
    pub command: String,
    pub now_ms: u64,
    pub plan_id: String,
    pub plan_revision: u64,
    pub account_id: String,
    pub connection_id: String,
    pub business_endpoint_id: String,
    pub provider_subject_id: String,
    pub read_set: SimulatorReadSet,
    pub authority: SimulatorAuthority,
    pub provider_policy: SimulatorProviderPolicy,
    pub scripted_outcome: ScriptedOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum PassReason {
    #[serde(rename = "NOT_RELEVANT")]
    NotRelevant,
    #[serde(rename = "LOCATION")]
    Location,
    #[serde(rename = "COMPENSATION")]
    Compensation,
    #[serde(rename = "SENIORITY")]
    Seniority,
    #[serde(rename = "EMPLOYMENT_TYPE")]
    EmploymentType,
    #[serde(rename = "OTHER_ROLE")]
    OtherRole,
}

impl PassReason {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "NOT_RELEVANT" => Some(Self::NotRelevant),
            "LOCATION" => Some(Self::Location),
            "COMPENSATION" => Some(Self::Compensation),
            "SENIORITY" => Some(Self::Seniority),
            "EMPLOYMENT_TYPE" => Some(Self::EmploymentType),
            "OTHER_ROLE" => Some(Self::OtherRole),
            _ => None,
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::NotRelevant => "NOT_RELEVANT",
            Self::Location => "LOCATION",
            Self::Compensation => "COMPENSATION",
            Self::Seniority => "SENIORITY",
            Self::EmploymentType => "EMPLOYMENT_TYPE",
            Self::OtherRole => "OTHER_ROLE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentKind {
    Help,
    Status,
    Matches,
    Show,
    Save,
    Pass,
    Prepare,
    Review,
    Approve,
    Cancel,
    Pause,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnerIntent {
    Help,
    Status,
    Matches { limit: Option<u8> },
    Show { job_ref: String },
    Save { job_ref: String },
    Pass { job_ref: String, reason: PassReason },
    Prepare { job_ref: String },
    Review { plan_ref: String },
    Approve { plan_ref: String },
    Cancel { plan_ref: String },
    Pause,
    Stop,
}

impl OwnerIntent {
    fn kind(&self) -> IntentKind {
        match self {
            Self::Help => IntentKind::Help,
            Self::Status => IntentKind::Status,
            Self::Matches { .. } => IntentKind::Matches,
            Self::Show { .. } => IntentKind::Show,
            Self::Save { .. } => IntentKind::Save,
            Self::Pass { .. } => IntentKind::Pass,
            Self::Prepare { .. } => IntentKind::Prepare,
            Self::Review { .. } => IntentKind::Review,
            Self::Approve { .. } => IntentKind::Approve,
            Self::Cancel { .. } => IntentKind::Cancel,
            Self::Pause => IntentKind::Pause,
            Self::Stop => IntentKind::Stop,
        }
    }

    fn normalized(&self) -> String {
        match self {
            Self::Help => "HELP".to_string(),
            Self::Status => "STATUS".to_string(),
            Self::Matches { limit: None } => "MATCHES".to_string(),
            Self::Matches { limit: Some(limit) } => format!("MATCHES {limit}"),
            Self::Show { job_ref } => format!("SHOW {job_ref}"),
            Self::Save { job_ref } => format!("SAVE {job_ref}"),
            Self::Pass { job_ref, reason } => {
                format!("PASS {job_ref} {}", reason.token())
            }
            Self::Prepare { job_ref } => format!("PREPARE {job_ref}"),
            Self::Review { plan_ref } => format!("REVIEW {plan_ref}"),
            Self::Approve { plan_ref } => format!("APPROVE {plan_ref}"),
            Self::Cancel { plan_ref } => format!("CANCEL {plan_ref}"),
            Self::Pause => "PAUSE".to_string(),
            Self::Stop => "STOP".to_string(),
        }
    }

    fn arguments(&self) -> CanonicalArguments {
        match self {
            Self::Matches { limit } => CanonicalArguments {
                limit: *limit,
                ..CanonicalArguments::default()
            },
            Self::Show { job_ref } | Self::Save { job_ref } | Self::Prepare { job_ref } => {
                CanonicalArguments {
                    job_ref: Some(job_ref.clone()),
                    ..CanonicalArguments::default()
                }
            }
            Self::Pass { job_ref, reason } => CanonicalArguments {
                job_ref: Some(job_ref.clone()),
                pass_reason: Some(*reason),
                ..CanonicalArguments::default()
            },
            Self::Review { plan_ref } | Self::Approve { plan_ref } | Self::Cancel { plan_ref } => {
                CanonicalArguments {
                    plan_ref: Some(plan_ref.clone()),
                    ..CanonicalArguments::default()
                }
            }
            _ => CanonicalArguments::default(),
        }
    }

    fn plan_action(&self) -> Option<PlanAction> {
        match self {
            Self::Save { .. } => Some(PlanAction::Save),
            Self::Pass { .. } => Some(PlanAction::Pass),
            Self::Prepare { .. } => Some(PlanAction::Prepare),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionCode {
    InvalidInput,
    Unsupported,
    PersonalWhatsappUnsupported,
    PersonalImessageBackgroundUnsupported,
    NonSyntheticIdentifier,
    ProviderPolicyMismatch,
    AuthorityDenied,
    Suppressed,
    PlanReferenceMismatch,
    ActionTruthCeiling,
    ProviderTruthCeiling,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulatorError {
    pub code: RejectionCode,
}

impl SimulatorError {
    fn new(code: RejectionCode) -> Self {
        Self { code }
    }
}

impl fmt::Display for SimulatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "business messaging simulator rejected: {:?}",
            self.code
        )
    }
}

impl std::error::Error for SimulatorError {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalArguments {
    pub job_ref: Option<String>,
    pub limit: Option<u8>,
    pub pass_reason: Option<PassReason>,
    pub plan_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalIntent {
    pub version: u8,
    pub grammar_version: &'static str,
    pub kind: IntentKind,
    pub normalized: String,
    pub arguments: CanonicalArguments,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Save,
    Pass,
    Prepare,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredStepUp {
    None,
    AuthenticatedWeb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulatedPlanState {
    Simulated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalPlan {
    pub version: u8,
    pub account_id: String,
    pub action: PlanAction,
    pub authority: SimulatorAuthority,
    pub business_endpoint_id: String,
    pub command_sha256: String,
    pub connection_id: String,
    pub expires_at_ms: u64,
    pub plan_id: String,
    pub plan_revision: u64,
    pub provider: BusinessProvider,
    pub provider_policy: SimulatorProviderPolicy,
    pub provider_subject_id: String,
    pub read_set: SimulatorReadSet,
    pub required_step_up: RequiredStepUp,
    pub state: SimulatedPlanState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    RequestNotStarted,
    RequestStarted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalOperation {
    pub version: u8,
    pub action: PlanAction,
    pub adapter_release_revision: String,
    pub operation_key_sha256: String,
    pub plan_id: String,
    pub plan_revision: u64,
    pub plan_sha256: String,
    pub provider: BusinessProvider,
    pub provider_policy_revision: String,
    pub request_state: RequestState,
    pub scripted_outcome: ScriptedOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptAssertion {
    SimulatedNoEffect,
    StepUpRequired,
    Stopped,
    Paused,
    DeniedPreEffect,
    ProviderAccepted,
    Delivered,
    Read,
    FailedPreEffect,
    SideEffectUnknown,
    ProviderClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ZeroEffectAudit {
    browser_attempts: u8,
    credential_reads: u8,
    external_writes: u8,
    jobs_mutations: u8,
    network_attempts: u8,
    process_attempts: u8,
    provider_attempts: u8,
}

impl ZeroEffectAudit {
    pub const ZERO: Self = Self {
        browser_attempts: 0,
        credential_reads: 0,
        external_writes: 0,
        jobs_mutations: 0,
        network_attempts: 0,
        process_attempts: 0,
        provider_attempts: 0,
    };

    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalReceipt {
    pub version: u8,
    pub assertion: ReceiptAssertion,
    pub command_sha256: String,
    pub operation_sha256: Option<String>,
    pub plan_sha256: Option<String>,
    pub provider: BusinessProvider,
    pub provider_evidence_revision: Option<String>,
    pub request_started: bool,
    pub retry_allowed: bool,
    simulated: bool,
    pub terminal_at_ms: u64,
    pub zero_effect_audit: ZeroEffectAudit,
}

impl CanonicalReceipt {
    pub fn is_simulated(&self) -> bool {
        self.simulated
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalArtifact<T> {
    pub value: T,
    pub canonical: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationSuccess {
    pub command: CanonicalArtifact<CanonicalIntent>,
    pub plan: Option<CanonicalArtifact<CanonicalPlan>>,
    pub operation: Option<CanonicalArtifact<CanonicalOperation>>,
    pub receipt: CanonicalArtifact<CanonicalReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OperationKeyProjection {
    account_id: String,
    action: PlanAction,
    adapter_release_revision: String,
    business_endpoint_id: String,
    connection_revision: String,
    consent_revision: String,
    plan_id: String,
    plan_revision: u64,
    plan_sha256: String,
    provider: BusinessProvider,
    provider_policy_revision: String,
    provider_subject_id: String,
}

#[derive(Clone, Copy)]
struct OutcomeProjection {
    assertion: ReceiptAssertion,
    request_started: bool,
    retry_allowed: bool,
    has_provider_evidence: bool,
}

pub fn parse_owner_intent(raw: &str) -> Result<OwnerIntent, RejectionCode> {
    if raw.len() <= 256 && raw.eq_ignore_ascii_case("STOP") {
        return Ok(OwnerIntent::Stop);
    }
    if raw.is_empty()
        || raw.len() > 256
        || !raw.is_ascii()
        || raw.bytes().any(|byte| !(b' '..=b'~').contains(&byte))
    {
        return Err(RejectionCode::Unsupported);
    }

    let parts: Vec<&str> = raw.split(' ').collect();
    let Some(verb) = parts.first() else {
        return Err(RejectionCode::Unsupported);
    };
    if verb.is_empty() || !verb.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return Err(RejectionCode::Unsupported);
    }
    let upper_verb = verb.to_ascii_uppercase();
    match (upper_verb.as_str(), parts.as_slice()) {
        ("HELP", [_]) => Ok(OwnerIntent::Help),
        ("STATUS", [_]) => Ok(OwnerIntent::Status),
        ("MATCHES", [_]) => Ok(OwnerIntent::Matches { limit: None }),
        ("MATCHES", [_, limit]) if matches!(*limit, "1" | "2" | "3" | "4" | "5") => {
            Ok(OwnerIntent::Matches {
                limit: Some(limit.as_bytes()[0] - b'0'),
            })
        }
        ("SHOW", [_, job_ref]) if valid_ref(job_ref, "J-") => Ok(OwnerIntent::Show {
            job_ref: (*job_ref).to_string(),
        }),
        ("SAVE", [_, job_ref]) if valid_ref(job_ref, "J-") => Ok(OwnerIntent::Save {
            job_ref: (*job_ref).to_string(),
        }),
        ("PASS", [_, job_ref, reason]) if valid_ref(job_ref, "J-") => {
            let reason = PassReason::parse(reason).ok_or(RejectionCode::Unsupported)?;
            Ok(OwnerIntent::Pass {
                job_ref: (*job_ref).to_string(),
                reason,
            })
        }
        ("PREPARE", [_, job_ref]) if valid_ref(job_ref, "J-") => Ok(OwnerIntent::Prepare {
            job_ref: (*job_ref).to_string(),
        }),
        ("REVIEW", [_, plan_ref]) if valid_ref(plan_ref, "P-") => Ok(OwnerIntent::Review {
            plan_ref: (*plan_ref).to_string(),
        }),
        ("APPROVE", [_, plan_ref]) if valid_ref(plan_ref, "P-") => Ok(OwnerIntent::Approve {
            plan_ref: (*plan_ref).to_string(),
        }),
        ("CANCEL", [_, plan_ref]) if valid_ref(plan_ref, "P-") => Ok(OwnerIntent::Cancel {
            plan_ref: (*plan_ref).to_string(),
        }),
        ("PAUSE", [_]) => Ok(OwnerIntent::Pause),
        _ => Err(RejectionCode::Unsupported),
    }
}

pub fn reject_unsupported_channel(value: &str) -> Option<RejectionCode> {
    match value {
        "whatsapp"
        | "whatsapp_personal"
        | "personal_whatsapp"
        | "whatsapp_web"
        | "whatsapp_qr_session"
        | "qr_device_session" => Some(RejectionCode::PersonalWhatsappUnsupported),
        "imessage" | "personal_imessage" | "sms" | "background_sms" => {
            Some(RejectionCode::PersonalImessageBackgroundUnsupported)
        }
        _ => None,
    }
}

pub fn simulate_business_messaging(
    input: &SimulatorInput,
) -> Result<SimulationSuccess, SimulatorError> {
    validate_input(input)?;
    let intent = parse_owner_intent(&input.command).map_err(SimulatorError::new)?;
    let command = artifact(CanonicalIntent {
        version: SIMULATOR_VERSION,
        grammar_version: GRAMMAR_VERSION,
        kind: intent.kind(),
        normalized: intent.normalized(),
        arguments: intent.arguments(),
    })?;

    if matches!(intent, OwnerIntent::Stop) {
        return no_plan_success(input, command, ReceiptAssertion::Stopped);
    }
    validate_provider_policy(input)?;
    if input.authority.suppression_active {
        return Err(SimulatorError::new(RejectionCode::Suppressed));
    }
    if input.authority.kill_switch_active {
        return Err(SimulatorError::new(RejectionCode::AuthorityDenied));
    }

    match &intent {
        OwnerIntent::Approve { plan_ref }
        | OwnerIntent::Review { plan_ref }
        | OwnerIntent::Cancel { plan_ref } => {
            if plan_ref != &input.plan_id {
                return Err(SimulatorError::new(RejectionCode::PlanReferenceMismatch));
            }
            let assertion = match intent {
                OwnerIntent::Approve { .. } => ReceiptAssertion::StepUpRequired,
                OwnerIntent::Cancel { .. } => ReceiptAssertion::DeniedPreEffect,
                _ => ReceiptAssertion::SimulatedNoEffect,
            };
            return no_plan_success(input, command, assertion);
        }
        _ => {}
    }

    if matches!(intent, OwnerIntent::Pause) {
        return no_plan_success(input, command, ReceiptAssertion::Paused);
    }

    let Some(action) = intent.plan_action() else {
        return no_plan_success(input, command, ReceiptAssertion::SimulatedNoEffect);
    };
    if !all_positive_authority(&input.authority) {
        return Err(SimulatorError::new(RejectionCode::AuthorityDenied));
    }
    if action == PlanAction::Prepare && input.scripted_outcome != ScriptedOutcome::SimulatedNoEffect
    {
        return Err(SimulatorError::new(RejectionCode::ActionTruthCeiling));
    }

    let expires_at_ms = input
        .now_ms
        .checked_add(300_000)
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or_else(|| SimulatorError::new(RejectionCode::InvalidInput))?;
    let required_step_up = if action == PlanAction::Prepare {
        RequiredStepUp::AuthenticatedWeb
    } else {
        RequiredStepUp::None
    };
    let plan = artifact(CanonicalPlan {
        version: SIMULATOR_VERSION,
        account_id: input.account_id.clone(),
        action,
        authority: input.authority.clone(),
        business_endpoint_id: input.business_endpoint_id.clone(),
        command_sha256: command.sha256.clone(),
        connection_id: input.connection_id.clone(),
        expires_at_ms,
        plan_id: input.plan_id.clone(),
        plan_revision: input.plan_revision,
        provider: input.provider,
        provider_policy: input.provider_policy.clone(),
        provider_subject_id: input.provider_subject_id.clone(),
        read_set: input.read_set.clone(),
        required_step_up,
        state: SimulatedPlanState::Simulated,
    })?;

    let outcome = project_outcome(input)?;
    let operation_key = artifact(OperationKeyProjection {
        account_id: input.account_id.clone(),
        action,
        adapter_release_revision: input.read_set.adapter_release_revision.clone(),
        business_endpoint_id: input.business_endpoint_id.clone(),
        connection_revision: input.read_set.connection_revision.clone(),
        consent_revision: input.read_set.consent_revision.clone(),
        plan_id: input.plan_id.clone(),
        plan_revision: input.plan_revision,
        plan_sha256: plan.sha256.clone(),
        provider: input.provider,
        provider_policy_revision: input.read_set.provider_policy_revision.clone(),
        provider_subject_id: input.provider_subject_id.clone(),
    })?;
    let request_state = if outcome.request_started {
        RequestState::RequestStarted
    } else {
        RequestState::RequestNotStarted
    };
    let operation = artifact(CanonicalOperation {
        version: SIMULATOR_VERSION,
        action,
        adapter_release_revision: input.read_set.adapter_release_revision.clone(),
        operation_key_sha256: operation_key.sha256,
        plan_id: input.plan_id.clone(),
        plan_revision: input.plan_revision,
        plan_sha256: plan.sha256.clone(),
        provider: input.provider,
        provider_policy_revision: input.read_set.provider_policy_revision.clone(),
        request_state,
        scripted_outcome: input.scripted_outcome,
    })?;
    let receipt = make_receipt(
        input,
        command.sha256.clone(),
        outcome.assertion,
        Some(plan.sha256.clone()),
        Some(operation.sha256.clone()),
        outcome.request_started,
        outcome.retry_allowed,
        outcome.has_provider_evidence,
    )?;
    Ok(SimulationSuccess {
        command,
        plan: Some(plan),
        operation: Some(operation),
        receipt,
    })
}

pub fn simulate_business_messaging_value(
    value: Value,
) -> Result<SimulationSuccess, SimulatorError> {
    if let Some(provider) = value
        .as_object()
        .and_then(|object| object.get("provider"))
        .and_then(Value::as_str)
    {
        if let Some(code) = reject_unsupported_channel(provider) {
            return Err(SimulatorError::new(code));
        }
    }
    let input: SimulatorInput = serde_json::from_value(value)
        .map_err(|_| SimulatorError::new(RejectionCode::InvalidInput))?;
    simulate_business_messaging(&input)
}

fn no_plan_success(
    input: &SimulatorInput,
    command: CanonicalArtifact<CanonicalIntent>,
    assertion: ReceiptAssertion,
) -> Result<SimulationSuccess, SimulatorError> {
    let receipt = make_receipt(
        input,
        command.sha256.clone(),
        assertion,
        None,
        None,
        false,
        false,
        false,
    )?;
    Ok(SimulationSuccess {
        command,
        plan: None,
        operation: None,
        receipt,
    })
}

#[allow(clippy::too_many_arguments)]
fn make_receipt(
    input: &SimulatorInput,
    command_sha256: String,
    assertion: ReceiptAssertion,
    plan_sha256: Option<String>,
    operation_sha256: Option<String>,
    request_started: bool,
    retry_allowed: bool,
    has_provider_evidence: bool,
) -> Result<CanonicalArtifact<CanonicalReceipt>, SimulatorError> {
    let provider_evidence_revision =
        has_provider_evidence.then(|| input.provider_policy.status_evidence_revision.clone());
    artifact(CanonicalReceipt {
        version: SIMULATOR_VERSION,
        assertion,
        command_sha256,
        operation_sha256,
        plan_sha256,
        provider: input.provider,
        provider_evidence_revision,
        request_started,
        retry_allowed,
        simulated: true,
        terminal_at_ms: input.now_ms,
        zero_effect_audit: ZeroEffectAudit::ZERO,
    })
}

fn project_outcome(input: &SimulatorInput) -> Result<OutcomeProjection, SimulatorError> {
    let outcome = match input.scripted_outcome {
        ScriptedOutcome::SimulatedNoEffect => OutcomeProjection {
            assertion: ReceiptAssertion::SimulatedNoEffect,
            request_started: false,
            retry_allowed: false,
            has_provider_evidence: false,
        },
        ScriptedOutcome::FailedPreRequest => OutcomeProjection {
            assertion: ReceiptAssertion::FailedPreEffect,
            request_started: false,
            retry_allowed: true,
            has_provider_evidence: false,
        },
        ScriptedOutcome::TimeoutAfterRequestStart => OutcomeProjection {
            assertion: ReceiptAssertion::SideEffectUnknown,
            request_started: true,
            retry_allowed: false,
            has_provider_evidence: false,
        },
        ScriptedOutcome::ProviderClosed
            if input.provider_policy.status_evidence_revision
                != provider_close_evidence_revision(input.provider) =>
        {
            return Err(SimulatorError::new(RejectionCode::ProviderTruthCeiling));
        }
        ScriptedOutcome::ProviderClosed => OutcomeProjection {
            assertion: ReceiptAssertion::ProviderClosed,
            request_started: true,
            retry_allowed: false,
            has_provider_evidence: true,
        },
        ScriptedOutcome::ProviderAccepted
            if input.provider_policy.status_evidence_revision
                != provider_gateway_evidence_revision(input.provider) =>
        {
            return Err(SimulatorError::new(RejectionCode::ProviderTruthCeiling));
        }
        ScriptedOutcome::ProviderAccepted => OutcomeProjection {
            assertion: ReceiptAssertion::ProviderAccepted,
            request_started: true,
            retry_allowed: false,
            has_provider_evidence: true,
        },
        ScriptedOutcome::WhatsappDelivered | ScriptedOutcome::WhatsappRead
            if input.provider != BusinessProvider::WhatsappBusinessPlatform
                || input.provider_policy.status_evidence_revision
                    != WHATSAPP_STATUS_EVIDENCE_REVISION =>
        {
            return Err(SimulatorError::new(RejectionCode::ProviderTruthCeiling));
        }
        ScriptedOutcome::WhatsappDelivered => OutcomeProjection {
            assertion: ReceiptAssertion::Delivered,
            request_started: true,
            retry_allowed: false,
            has_provider_evidence: true,
        },
        ScriptedOutcome::WhatsappRead => OutcomeProjection {
            assertion: ReceiptAssertion::Read,
            request_started: true,
            retry_allowed: false,
            has_provider_evidence: true,
        },
    };
    Ok(outcome)
}

fn provider_gateway_evidence_revision(provider: BusinessProvider) -> &'static str {
    match provider {
        BusinessProvider::WhatsappBusinessPlatform => WHATSAPP_GATEWAY_EVIDENCE_REVISION,
        BusinessProvider::AppleMessagesForBusiness => APPLE_GATEWAY_EVIDENCE_REVISION,
    }
}

fn provider_close_evidence_revision(provider: BusinessProvider) -> &'static str {
    match provider {
        BusinessProvider::WhatsappBusinessPlatform => WHATSAPP_CLOSE_EVIDENCE_REVISION,
        BusinessProvider::AppleMessagesForBusiness => APPLE_CLOSE_EVIDENCE_REVISION,
    }
}

fn validate_input(input: &SimulatorInput) -> Result<(), SimulatorError> {
    if input.version != SIMULATOR_VERSION
        || input.now_ms == 0
        || input.now_ms > MAX_SAFE_INTEGER
        || input.plan_revision == 0
        || input.plan_revision > MAX_SAFE_INTEGER
        || !valid_ref(&input.plan_id, "P-")
    {
        return Err(SimulatorError::new(RejectionCode::InvalidInput));
    }
    if !valid_synthetic_id(&input.account_id, "acct_test_")
        || !valid_synthetic_id(&input.connection_id, "conn_test_")
        || !valid_synthetic_id(&input.business_endpoint_id, "endpoint_test_")
        || !valid_synthetic_id(&input.provider_subject_id, "subject_test_")
    {
        return Err(SimulatorError::new(RejectionCode::NonSyntheticIdentifier));
    }
    let read_set = &input.read_set;
    if !valid_synthetic_revision(&read_set.connection_revision, "connection")
        || !valid_synthetic_revision(&read_set.consent_revision, "consent")
        || !valid_synthetic_revision(&read_set.opt_out_parser_revision, "opt_out")
        || !valid_synthetic_revision(&read_set.command_parser_revision, "command")
        || !valid_synthetic_revision(&read_set.locale_table_revision, "locale")
        || !valid_synthetic_revision(&read_set.provider_policy_revision, "provider_policy")
        || !valid_synthetic_revision(
            &read_set.provider_eligibility_revision,
            "provider_eligibility",
        )
        || !valid_synthetic_revision(&read_set.jobs_workspace_revision, "workspace")
        || !valid_synthetic_revision(&read_set.career_track_revision, "track")
        || !valid_synthetic_revision(&read_set.original_source_revision, "source")
        || !valid_synthetic_revision(&read_set.job_integrity_revision, "integrity")
        || !valid_synthetic_revision(&read_set.adapter_release_revision, "adapter")
        || !valid_synthetic_revision(&read_set.kill_switch_revision, "kill_switch")
        || !valid_status_evidence_revision(&input.provider_policy.status_evidence_revision)
    {
        return Err(SimulatorError::new(RejectionCode::InvalidInput));
    }
    Ok(())
}

fn validate_provider_policy(input: &SimulatorInput) -> Result<(), SimulatorError> {
    let evidence = input.provider_policy.status_evidence_revision.as_str();
    let matches = match input.provider {
        BusinessProvider::WhatsappBusinessPlatform => {
            input.provider_policy.mode == ProviderPolicyMode::WhatsappCustomerServiceWindow
                && matches!(
                    evidence,
                    WHATSAPP_STATUS_EVIDENCE_REVISION
                        | WHATSAPP_GATEWAY_EVIDENCE_REVISION
                        | WHATSAPP_CLOSE_EVIDENCE_REVISION
                        | "rev_test_unpinned_status_v1"
                )
        }
        BusinessProvider::AppleMessagesForBusiness => {
            input.provider_policy.mode == ProviderPolicyMode::AppleActiveConversation
                && matches!(
                    evidence,
                    APPLE_GATEWAY_EVIDENCE_REVISION | APPLE_CLOSE_EVIDENCE_REVISION
                )
        }
    };
    if !matches {
        return Err(SimulatorError::new(RejectionCode::ProviderPolicyMismatch));
    }
    Ok(())
}

fn all_positive_authority(authority: &SimulatorAuthority) -> bool {
    authority.connection_active
        && authority.consent_active
        && authority.provider_eligible
        && authority.track_eligible
        && authority.source_verified
        && authority.integrity_verified
        && !authority.suppression_active
        && !authority.kill_switch_active
}

fn valid_ref(value: &str, prefix: &str) -> bool {
    let Some(body) = value.strip_prefix(prefix) else {
        return false;
    };
    (10..=26).contains(&body.len())
        && body.bytes().all(|byte| {
            byte.is_ascii_digit()
                || (b'A'..=b'H').contains(&byte)
                || (b'J'..=b'N').contains(&byte)
                || (b'P'..=b'T').contains(&byte)
                || (b'V'..=b'Z').contains(&byte)
        })
}

fn valid_synthetic_id(value: &str, prefix: &str) -> bool {
    match prefix {
        "acct_test_" => matches!(value, "acct_test_owner" | "acct_test_alternate"),
        "conn_test_" => matches!(value, "conn_test_primary" | "conn_test_secondary"),
        "endpoint_test_" => matches!(
            value,
            "endpoint_test_whatsapp" | "endpoint_test_apple" | "endpoint_test_alternate"
        ),
        "subject_test_" => matches!(value, "subject_test_owner" | "subject_test_alternate"),
        _ => false,
    }
}

fn valid_synthetic_revision(value: &str, stem: &str) -> bool {
    let prefix = format!("rev_test_{stem}_v");
    let Some(version) = value.strip_prefix(&prefix) else {
        return false;
    };
    (1..=3).contains(&version.len())
        && version.as_bytes()[0].is_ascii_digit()
        && version.as_bytes()[0] != b'0'
        && version.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_status_evidence_revision(value: &str) -> bool {
    matches!(
        value,
        WHATSAPP_STATUS_EVIDENCE_REVISION
            | WHATSAPP_GATEWAY_EVIDENCE_REVISION
            | WHATSAPP_CLOSE_EVIDENCE_REVISION
            | APPLE_GATEWAY_EVIDENCE_REVISION
            | APPLE_CLOSE_EVIDENCE_REVISION
            | "rev_test_unpinned_status_v1"
    )
}

fn artifact<T: Serialize>(value: T) -> Result<CanonicalArtifact<T>, SimulatorError> {
    let canonical = canonical_json(&value)?;
    let sha256 = sha256_hex(&canonical);
    Ok(CanonicalArtifact {
        value,
        canonical,
        sha256,
    })
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, SimulatorError> {
    let value = serde_json::to_value(value)
        .map_err(|_| SimulatorError::new(RejectionCode::InvalidInput))?;
    let mut result = String::new();
    write_canonical_value(&value, &mut result)?;
    result.push('\n');
    Ok(result)
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn write_canonical_value(value: &Value, output: &mut String) -> Result<(), SimulatorError> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => output.push_str(&value.to_string()),
        Value::String(value) => output.push_str(
            &serde_json::to_string(value)
                .map_err(|_| SimulatorError::new(RejectionCode::InvalidInput))?,
        ),
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_canonical_value(value, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut keys: Vec<&String> = values.keys().collect();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key)
                        .map_err(|_| SimulatorError::new(RejectionCode::InvalidInput))?,
                );
                output.push(':');
                write_canonical_value(&values[key], output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_recursively_and_appends_one_line_feed() {
        let value = serde_json::json!({"z": {"b": 2, "a": 1}, "a": [3, 2]});
        assert_eq!(
            canonical_json(&value).unwrap(),
            "{\"a\":[3,2],\"z\":{\"a\":1,\"b\":2}}\n"
        );
    }

    #[test]
    fn grammar_is_closed_and_stop_is_checked_first() {
        assert_eq!(parse_owner_intent("sToP"), Ok(OwnerIntent::Stop));
        assert_eq!(
            parse_owner_intent("MATCHES 6"),
            Err(RejectionCode::Unsupported)
        );
        assert_eq!(
            parse_owner_intent("SHOW J-0123456789 https://example.invalid"),
            Err(RejectionCode::Unsupported)
        );
        assert_eq!(
            parse_owner_intent("ＳＴＯＰ"),
            Err(RejectionCode::Unsupported)
        );
    }

    #[test]
    fn personal_channels_have_closed_typed_rejections() {
        assert_eq!(
            reject_unsupported_channel("whatsapp_qr_session"),
            Some(RejectionCode::PersonalWhatsappUnsupported)
        );
        assert_eq!(
            reject_unsupported_channel("personal_imessage"),
            Some(RejectionCode::PersonalImessageBackgroundUnsupported)
        );
    }

    #[test]
    fn effect_audit_is_exactly_zero() {
        assert!(ZeroEffectAudit::ZERO.is_zero());
    }
}
