pub mod agent_ui;
pub mod ai;
pub mod app_paths;
pub mod audio;
pub mod cards;
pub mod clock;
pub mod cloud;
pub mod config;
pub mod intelligence;
pub mod ipc;
pub mod ledger;
pub mod logging;
pub mod meeting;
pub mod observability;
pub mod overlay;
pub mod prestage;
pub mod session;
pub mod state;

pub mod overlay_ipc;
pub mod pcm;
pub mod stt;
pub mod vad;

pub use agent_ui::{AgentConnectorInfo, AgentSessionSummary, AgentSummary};
pub use ai::{
    AiCapabilities, AiCapability, AiModelId, AiProviderId, AiProviderKind, AiRuntimeStatus,
    AnswerContext, AnswerContextKind, AnswerRequest, AnswerResponse, AnswerStreamEvent, CostBudget,
    LatencyBudget, PrivacyFlags, ProviderClientConfig, ProviderRequestPayload, ProviderRoute,
    ProviderSelector, ProviderStatus, RouteBudget, RouteSelectionPolicy, SafetyFlags,
};
pub use audio::{
    AudioBackend, AudioCaptureConfig, AudioCapturePlan, AudioCaptureState, AudioCaptureStatus,
    AudioChunkMetadata, AudioDeviceDescriptor, AudioDeviceRole, AudioEvent, AudioMixStrategy,
    AudioPipelineStatus, AudioSourceConfig, AudioSourceKind, AudioSourcePlan, AudioSourceState,
    AudioSourceStatus, AudioStreamFormat, SttSegmentMetadata,
};
pub use cards::{CardArtifactType, CardKind, CueCard, CueCardArtifact};
pub use cloud::{
    ArtifactUploadMetadata, CloudAuthState, CloudDataScope, CloudDeviceId, CloudEndpointConfig,
    CloudEnvironment, CloudObjectKind, CloudSyncState, CloudSyncStatus, DeletionRequest,
    EmbeddingMetadata, EncryptionMetadata, ExportRequest, MemoryChunk, MemoryChunkKind,
    RagCitation, RagQuery, RagResult, RetentionPolicy, SyncDirection, SyncError, SyncEvent, UserId,
    WorkspaceId,
};
pub use config::{
    load_account, load_settings, save_account, save_settings, AccountConfig, CueSettings,
};
pub use intelligence::{
    analyze_segment, detect_for_me_question, generate_recap, local_answer, ForMeQuestion,
    SegmentAnalysis,
};
pub use ipc::{DaemonRequest, DaemonResponse};
pub use ledger::{
    parse_and_verify, LedgerItem, LedgerKind, LedgerState, DEFAULT_LEDGER_CAP, EXTRACTION_PROMPT,
};
pub use logging::{
    init_local_json_logging, local_log_dir, log_file_prefix, retain_recent_log_files, LocalLogGuard,
};
pub use meeting::{
    ActionItem, ContextArtifact, ContextKind, ContextProcessingStatus, ConversationTurn, Decision,
    MeetingRecap, MeetingRecord, MemoryHit, Speaker, TranscriptSegment,
};
pub use observability::{
    account_id_hash_prefix, new_request_id, new_trace_id, platform, sanitize_observability_id,
    trace_id_from_env, ObserveFields, BLUEY_REQUEST_ID_HEADER, BLUEY_TRACE_ID_ENV,
    BLUEY_TRACE_ID_HEADER,
};
pub use overlay::{
    AnswerStatusState, AnswerStatusStep, MeetingConversationTurn, MeetingTranscriptLine,
    OverlayCommand, OverlayContextItem, OverlayEvent, OverlayPosition, OverlaySessionItem,
};
pub use prestage::{build_prestage_brief, PrestageInput};
pub use state::{DaemonState, MeetingState};

#[macro_export]
macro_rules! observe {
    ($level:expr, $fields:expr, $message:literal $(,)?) => {{
        let fields = $fields;
        tracing::event!(
            $level,
            component = %fields.component,
            version = %fields.version,
            platform = %fields.platform,
            trace_id = %fields.trace_id_value(),
            request_id = %fields.request_id_value(),
            session_id = %fields.session_id_value(),
            account_id_hash = %fields.account_id_hash_value(),
            status = %fields.status_value(),
            latency_ms = fields.latency_ms,
            provider = %fields.provider_value(),
            model = %fields.model_value(),
            cost_cents_to_customer = fields.cost_cents_to_customer,
            cost_cents_to_bluey = fields.cost_cents_to_bluey,
            $message
        );
    }};
}
