use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::clock;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(String);

impl WorkspaceId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Self::default()
        } else {
            Self(trimmed.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self("default".to_string())
    }
}

impl From<&str> for WorkspaceId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for WorkspaceId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(String);

impl UserId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Self::default()
        } else {
            Self(trimmed.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self("local-user".to_string())
    }
}

impl From<&str> for UserId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for UserId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CloudDeviceId(String);

impl CloudDeviceId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Self::default()
        } else {
            Self(trimmed.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CloudDeviceId {
    fn default() -> Self {
        Self("local-device".to_string())
    }
}

impl From<&str> for CloudDeviceId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for CloudDeviceId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for CloudDeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudEnvironment {
    Local,
    Development,
    Staging,
    Production,
}

impl Default for CloudEnvironment {
    fn default() -> Self {
        Self::Development
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudEndpointConfig {
    pub api_url: String,
    pub environment: CloudEnvironment,
}

impl CloudEndpointConfig {
    pub fn new(api_url: impl Into<String>, environment: CloudEnvironment) -> Self {
        Self {
            api_url: api_url.into(),
            environment,
        }
    }

    pub fn local() -> Self {
        Self::new("http://127.0.0.1:8787", CloudEnvironment::Local)
    }
}

impl Default for CloudEndpointConfig {
    fn default() -> Self {
        Self::local()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthState {
    SignedOut,
    TokenConfigured,
    Authenticated,
    Expired,
    Failed,
}

impl Default for CloudAuthState {
    fn default() -> Self {
        Self::SignedOut
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudSyncState {
    Disabled,
    Ready,
    Syncing,
    Degraded,
    Failed,
}

impl Default for CloudSyncState {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudDataScope {
    UserPrivate,
    Workspace,
    Organization,
}

impl Default for CloudDataScope {
    fn default() -> Self {
        Self::Workspace
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionMetadata {
    pub algorithm: String,
    pub key_id: String,
    pub nonce: Option<String>,
    pub ciphertext_sha256: Option<String>,
}

impl EncryptionMetadata {
    pub fn envelope(key_id: impl Into<String>) -> Self {
        Self {
            algorithm: "xchacha20_poly1305".to_string(),
            key_id: key_id.into(),
            nonce: None,
            ciphertext_sha256: None,
        }
    }

    pub fn with_nonce(mut self, nonce: impl Into<String>) -> Self {
        self.nonce = Some(nonce.into());
        self
    }

    pub fn with_ciphertext_sha256(mut self, ciphertext_sha256: impl Into<String>) -> Self {
        self.ciphertext_sha256 = Some(ciphertext_sha256.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadMetadata {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub meeting_id: Option<Uuid>,
    pub local_path: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
    pub scope: CloudDataScope,
    pub encryption: Option<EncryptionMetadata>,
    pub created_at: String,
}

impl ArtifactUploadMetadata {
    pub fn new(
        workspace_id: impl Into<WorkspaceId>,
        local_path: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            meeting_id: None,
            local_path: local_path.into(),
            content_type: None,
            size_bytes,
            scope: CloudDataScope::default(),
            encryption: None,
            created_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn with_meeting_id(mut self, meeting_id: Uuid) -> Self {
        self.meeting_id = Some(meeting_id);
        self
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    pub fn with_encryption(mut self, encryption: EncryptionMetadata) -> Self {
        self.encryption = Some(encryption);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub occurred_at: String,
}

impl SyncError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            occurred_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub retain_transcripts_days: Option<u32>,
    pub retain_artifacts_days: Option<u32>,
    pub retain_embeddings_days: Option<u32>,
    pub delete_on_workspace_close: bool,
}

impl RetentionPolicy {
    pub fn managed_default() -> Self {
        Self {
            retain_transcripts_days: Some(365),
            retain_artifacts_days: Some(365),
            retain_embeddings_days: Some(365),
            delete_on_workspace_close: true,
        }
    }
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self::managed_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudSyncStatus {
    pub endpoint: CloudEndpointConfig,
    pub auth_state: CloudAuthState,
    pub sync_state: CloudSyncState,
    pub workspace_id: WorkspaceId,
    pub user_id: UserId,
    pub device_id: CloudDeviceId,
    pub rag_enabled: bool,
    pub pending_uploads: usize,
    pub pending_downloads: usize,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
    pub retention: RetentionPolicy,
    pub updated_at: String,
}

impl CloudSyncStatus {
    pub fn disabled(message: impl Into<String>) -> Self {
        Self {
            endpoint: CloudEndpointConfig::default(),
            auth_state: CloudAuthState::SignedOut,
            sync_state: CloudSyncState::Disabled,
            workspace_id: WorkspaceId::default(),
            user_id: UserId::default(),
            device_id: CloudDeviceId::default(),
            rag_enabled: false,
            pending_uploads: 0,
            pending_downloads: 0,
            last_synced_at: None,
            last_error: Some(message.into()),
            retention: RetentionPolicy::default(),
            updated_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn ready(
        endpoint: CloudEndpointConfig,
        workspace_id: impl Into<WorkspaceId>,
        user_id: impl Into<UserId>,
    ) -> Self {
        Self {
            endpoint,
            auth_state: CloudAuthState::TokenConfigured,
            sync_state: CloudSyncState::Ready,
            workspace_id: workspace_id.into(),
            user_id: user_id.into(),
            device_id: CloudDeviceId::default(),
            rag_enabled: true,
            pending_uploads: 0,
            pending_downloads: 0,
            last_synced_at: None,
            last_error: None,
            retention: RetentionPolicy::default(),
            updated_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn mark_syncing(&mut self) {
        self.sync_state = CloudSyncState::Syncing;
        self.updated_at = clock::now_epoch_ms_string();
    }

    pub fn with_device_id(mut self, device_id: impl Into<CloudDeviceId>) -> Self {
        self.device_id = device_id.into();
        self
    }

    pub fn mark_synced(&mut self) {
        let now = clock::now_epoch_ms_string();
        self.sync_state = CloudSyncState::Ready;
        self.pending_uploads = 0;
        self.pending_downloads = 0;
        self.last_synced_at = Some(now.clone());
        self.last_error = None;
        self.updated_at = now;
    }

    pub fn mark_failed(&mut self, message: impl Into<String>) {
        self.sync_state = CloudSyncState::Failed;
        self.auth_state = CloudAuthState::Failed;
        self.last_error = Some(message.into());
        self.updated_at = clock::now_epoch_ms_string();
    }

    pub fn mark_degraded(&mut self, message: impl Into<String>) {
        self.sync_state = CloudSyncState::Degraded;
        self.last_error = Some(message.into());
        self.updated_at = clock::now_epoch_ms_string();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    Upload,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudObjectKind {
    Meeting,
    TranscriptSegment,
    ContextArtifact,
    ActionItem,
    Decision,
    Recap,
    MemoryChunk,
    AnswerInstructions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub id: Uuid,
    pub direction: SyncDirection,
    pub object_kind: CloudObjectKind,
    pub object_id: String,
    pub workspace_id: WorkspaceId,
    pub created_at: String,
}

impl SyncEvent {
    pub fn upload(
        object_kind: CloudObjectKind,
        object_id: impl Into<String>,
        workspace_id: impl Into<WorkspaceId>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            direction: SyncDirection::Upload,
            object_kind,
            object_id: object_id.into(),
            workspace_id: workspace_id.into(),
            created_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryChunkKind {
    Transcript,
    Summary,
    ActionItem,
    Decision,
    Document,
    ScreenshotOcr,
    UserInstruction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    pub provider: String,
    pub model: String,
    pub dimensions: u16,
    pub created_at: String,
}

impl EmbeddingMetadata {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, dimensions: u16) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            dimensions,
            created_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryChunk {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub meeting_id: Option<Uuid>,
    pub kind: MemoryChunkKind,
    pub text: String,
    pub source: Option<String>,
    pub embedding: Option<EmbeddingMetadata>,
    pub created_at: String,
}

impl MemoryChunk {
    pub fn new(
        workspace_id: impl Into<WorkspaceId>,
        kind: MemoryChunkKind,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            meeting_id: None,
            kind,
            text: text.into(),
            source: None,
            embedding: None,
            created_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn with_meeting_id(mut self, meeting_id: Uuid) -> Self {
        self.meeting_id = Some(meeting_id);
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_embedding(mut self, embedding: EmbeddingMetadata) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RagQuery {
    pub workspace_id: WorkspaceId,
    pub query: String,
    pub limit: usize,
    pub include_sources: Vec<MemoryChunkKind>,
}

impl RagQuery {
    pub fn new(workspace_id: impl Into<WorkspaceId>, query: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            query: query.into(),
            limit: 8,
            include_sources: Vec::new(),
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit.clamp(1, 50);
        self
    }

    pub fn include(mut self, kind: MemoryChunkKind) -> Self {
        if !self.include_sources.contains(&kind) {
            self.include_sources.push(kind);
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagCitation {
    pub chunk_id: Uuid,
    pub title: Option<String>,
    pub source: Option<String>,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RagResult {
    pub query: RagQuery,
    pub answer_context: String,
    pub citations: Vec<RagCitation>,
    pub created_at: String,
}

impl RagResult {
    pub fn empty(query: RagQuery) -> Self {
        Self {
            query,
            answer_context: String::new(),
            citations: Vec::new(),
            created_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionRequest {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub object_kind: Option<CloudObjectKind>,
    pub object_id: Option<String>,
    pub requested_at: String,
}

impl DeletionRequest {
    pub fn workspace(workspace_id: impl Into<WorkspaceId>) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            object_kind: None,
            object_id: None,
            requested_at: clock::now_epoch_ms_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportRequest {
    pub id: Uuid,
    pub workspace_id: WorkspaceId,
    pub requested_by: UserId,
    pub include_artifacts: bool,
    pub requested_at: String,
}

impl ExportRequest {
    pub fn new(workspace_id: impl Into<WorkspaceId>, requested_by: impl Into<UserId>) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id: workspace_id.into(),
            requested_by: requested_by.into(),
            include_artifacts: true,
            requested_at: clock::now_epoch_ms_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_cloud_status_enables_rag() {
        let status = CloudSyncStatus::ready(
            CloudEndpointConfig::new("https://api.example.test", CloudEnvironment::Staging),
            "workspace-1",
            "user-1",
        );

        assert_eq!(status.sync_state, CloudSyncState::Ready);
        assert_eq!(status.auth_state, CloudAuthState::TokenConfigured);
        assert!(status.rag_enabled);
    }

    #[test]
    fn rag_query_limit_is_clamped() {
        let query = RagQuery::new("workspace-1", "launch risk")
            .with_limit(500)
            .include(MemoryChunkKind::Transcript)
            .include(MemoryChunkKind::Transcript);

        assert_eq!(query.limit, 50);
        assert_eq!(query.include_sources, vec![MemoryChunkKind::Transcript]);
    }

    #[test]
    fn memory_chunk_tracks_embedding_metadata() {
        let chunk = MemoryChunk::new("workspace-1", MemoryChunkKind::Summary, "Decision summary")
            .with_source("meeting recap")
            .with_embedding(EmbeddingMetadata::new("bluey", "embedding-small", 1536));

        assert_eq!(chunk.workspace_id.as_str(), "workspace-1");
        assert!(chunk.embedding.is_some());
        assert_eq!(chunk.source.as_deref(), Some("meeting recap"));
    }

    #[test]
    fn artifact_upload_metadata_keeps_encryption_boundary() {
        let upload = ArtifactUploadMetadata::new("workspace-1", "/tmp/capture.png", 4096)
            .with_content_type("image/png")
            .with_encryption(
                EncryptionMetadata::envelope("workspace-key")
                    .with_nonce("nonce")
                    .with_ciphertext_sha256("hash"),
            );

        assert_eq!(upload.scope, CloudDataScope::Workspace);
        assert_eq!(upload.content_type.as_deref(), Some("image/png"));
        assert!(upload.encryption.is_some());
    }
}
