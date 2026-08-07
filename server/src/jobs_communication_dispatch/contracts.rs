use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CommunicationProviderRequest {
    pub(crate) action_id: String,
    pub(crate) provider: String,
    pub(crate) provider_operation_key: String,
    pub(crate) payload_sha256: String,
    pub(crate) payload: Value,
    pub(crate) source_message: Option<CommunicationSourceMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommunicationSourceMessage {
    pub(crate) provider: String,
    pub(crate) provider_id: String,
    pub(crate) external_id: String,
    pub(crate) rfc_message_id: String,
    pub(crate) thread_id: String,
    pub(crate) conversation_id: String,
    pub(crate) sender: String,
    pub(crate) reply_target: String,
    pub(crate) subject: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderCommitEvidence {
    pub(crate) provider_object_id: String,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderFailureKind {
    Authorization,
    InvalidAction,
    ProviderRejected,
    RetryableNoSideEffect,
}

impl ProviderFailureKind {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Authorization => "provider_authorization_required",
            Self::InvalidAction => "provider_action_invalid",
            Self::ProviderRejected => "provider_rejected",
            Self::RetryableNoSideEffect => "provider_retryable_no_side_effect",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProviderDispatchResult {
    Committed(ProviderCommitEvidence),
    DefinitiveNoSideEffect {
        kind: ProviderFailureKind,
        retry_after_ms: Option<i64>,
    },
    Ambiguous {
        reason_code: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProviderLookupResult {
    Found(ProviderCommitEvidence),
    Absent,
    Inconclusive { reason_code: &'static str },
    Conflict,
}
