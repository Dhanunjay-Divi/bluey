pub mod ai;
pub mod app_paths;
pub mod assistant;
pub mod audio;
pub mod cards;
pub mod clock;
pub mod cloud;
pub mod config;
pub mod intelligence;
pub mod ipc;
pub mod ipc_auth;
pub mod jobs_handoff;
pub mod legal;
pub mod logging;
pub mod meeting;
pub mod observability;
pub mod overlay;
pub mod session;
pub mod state;

pub mod overlay_ipc;
pub mod pcm;
pub mod stt;
pub mod vad;
pub mod workspace;

pub use ai::{
    AiCapabilities, AiCapability, AiModelId, AiProviderId, AiProviderKind, AiRuntimeStatus,
    AnswerContext, AnswerContextKind, AnswerRequest, AnswerResponse, AnswerStreamEvent, CostBudget,
    LatencyBudget, PrivacyFlags, ProviderClientConfig, ProviderRequestPayload, ProviderRoute,
    ProviderSelector, ProviderStatus, RouteBudget, RouteSelectionPolicy, SafetyFlags,
};
pub use assistant::{AssistantMode, AssistantProfile, AssistantSourceReference};
pub use audio::{
    AudioBackend, AudioCaptureConfig, AudioCapturePlan, AudioCaptureState, AudioCaptureStatus,
    AudioChunkMetadata, AudioDeviceDescriptor, AudioDeviceRole, AudioEvent, AudioMixStrategy,
    AudioPipelineStatus, AudioReadinessProbeResult, AudioReadinessSourceResult,
    AudioReadinessState, AudioSourceConfig, AudioSourceKind, AudioSourcePlan, AudioSourceState,
    AudioSourceStatus, AudioStreamFormat, SttSegmentMetadata, AUDIO_READINESS_SCHEMA_VERSION,
};
pub use cards::{CardArtifactType, CardKind, CueCard, CueCardArtifact, CueCardAttachment};
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
pub use intelligence::{analyze_segment, generate_recap, local_answer, SegmentAnalysis};
pub use ipc::{
    DaemonRequest, DaemonResponse, ScreenshotContextAttachReceipt, ScreenshotContextAttachRequest,
};
pub use ipc_auth::{
    ipc_capability_path, load_ipc_capability, publish_ipc_capability,
    remove_ipc_capability_if_current, validated_loopback_ipc_addr, AuthenticatedDaemonRequest,
    DaemonWireRequest, IpcAuthErrorCode, IpcAuthorization, IpcBearer, IpcCapabilityRecord,
    IpcEnvelopeType,
};
#[cfg(windows)]
pub use ipc_auth::{
    validate_windows_named_pipe_client, validate_windows_named_pipe_server,
    windows_named_pipe_name, WindowsOwnerOnlySecurity, WINDOWS_IPC_PIPE_PREFIX,
};
pub use jobs_handoff::{
    JobsHandoffImportAuthorization, JobsHandoffImportReceipt, JobsHandoffImportRequest,
    BLUEY_JOBS_EVIDENCE_SOURCE,
};
pub use legal::{
    embedded_product_policy, EmbeddedProductPolicy, BLUEY_BUILD_ID, BLUEY_LICENSE_ID,
    BLUEY_POLICY_SCHEMA_VERSION, BLUEY_TERMS_URL,
};
pub use logging::{
    init_local_json_logging, local_log_dir, log_file_prefix, retain_recent_log_files, LocalLogGuard,
};
pub use meeting::{
    short_session_code, ActionItem, ContextArtifact, ContextCloudSyncPolicy, ContextKind,
    ContextProcessingStatus, ConversationTurn, Decision, MeetingDiagnostics, MeetingRecap,
    MeetingRecord, MemoryHit, Speaker, TranscriptSegment,
};
pub use observability::{
    account_id_hash_prefix, new_request_id, new_trace_id, platform, sanitize_observability_id,
    short_observability_ref, trace_id_from_env, ObserveFields, BLUEY_REQUEST_ID_HEADER,
    BLUEY_TRACE_ID_ENV, BLUEY_TRACE_ID_HEADER,
};
pub use overlay::{
    OverlayCommand, OverlayContextItem, OverlayEvent, OverlayPosition, OverlaySessionItem,
};
pub use state::{DaemonState, MeetingState};
pub use workspace::{
    WorkspaceActivityReference, WorkspaceArtifactReference, WorkspaceContextReference,
    WorkspaceCreateRequest, WorkspaceDeletionState, WorkspaceInstructionsPatch,
    WorkspaceLinkedJobMetadata, WorkspaceRecord, WorkspaceUpdateRequest,
};

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
