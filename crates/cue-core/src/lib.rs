pub mod ai;
pub mod app_paths;
pub mod audio;
pub mod cards;
pub mod clock;
pub mod cloud;
pub mod config;
pub mod intelligence;
pub mod ipc;
pub mod meeting;
pub mod overlay;
pub mod session;
pub mod state;

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
pub use cards::{CardKind, CueCard};
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
pub use ipc::{DaemonRequest, DaemonResponse};
pub use meeting::{
    ActionItem, ContextArtifact, ContextKind, ContextProcessingStatus, ConversationTurn, Decision,
    MeetingRecap, MeetingRecord, MemoryHit, Speaker, TranscriptSegment,
};
pub use overlay::{OverlayCommand, OverlayEvent, OverlayPosition};
pub use state::{DaemonState, MeetingState};
