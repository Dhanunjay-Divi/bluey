use serde::{Deserialize, Serialize};

use crate::{
    sanitize_observability_id, ActionItem, AiRuntimeStatus, AnswerRequest, AnswerResponse,
    AnswerStreamEvent, AssistantProfile, AudioPipelineStatus, AudioReadinessProbeResult,
    CloudSyncStatus, ContextArtifact, CueCard, DaemonState, MeetingRecap, MemoryHit,
    OverlayPosition, Speaker, WorkspaceCreateRequest, WorkspaceRecord, WorkspaceUpdateRequest,
};

pub const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:57321";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotContextAttachRequest {
    pub operation_id: uuid::Uuid,
    #[serde(default)]
    pub expected_owner_account_id: Option<String>,
    #[serde(default)]
    pub expected_session_id: Option<uuid::Uuid>,
    pub retained_path: String,
    pub content_sha256: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotContextAttachReceipt {
    pub operation_id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub artifact: ContextArtifact,
    pub already_attached: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    /// Backward-compatible trace envelope for UI/CLI -> daemon IPC.
    ///
    /// Older clients send the inner request directly. Newer clients wrap any
    /// request in this variant so daemon-side logs and downstream cloud calls
    /// can share the same trace id.
    WithTrace {
        trace_id: String,
        request: Box<DaemonRequest>,
    },
    Ping,
    Status,
    Shutdown,
    OverlayShow,
    OverlayHide,
    OverlayToggle,
    OverlayClear,
    OverlayBoot {
        title: String,
        lines: Vec<String>,
    },
    OverlaySetOpacity {
        opacity: f32,
    },
    OverlaySetPosition {
        position: OverlayPosition,
    },
    PushCard {
        card: CueCard,
    },
    MeetingStart {
        title: Option<String>,
    },
    MeetingEnd,
    TranscriptAdd {
        speaker: Speaker,
        text: String,
        is_final: bool,
    },
    Ask {
        question: String,
    },
    Answer {
        request: AnswerRequest,
    },
    ContextAdd {
        path: String,
        title: Option<String>,
        note: Option<String>,
    },
    ScreenshotContextDestination,
    ScreenshotContextAttach {
        request: ScreenshotContextAttachRequest,
    },
    ContextList,
    ActivePageCapture,
    ScreenCaptureStart {
        interval_secs: Option<u64>,
    },
    ScreenCaptureStop,
    InstructionsSet {
        text: String,
    },
    InstructionsGet,
    InstructionsClear,
    AssistantProfileGet,
    AssistantProfileSet {
        profile: AssistantProfile,
    },
    WorkspaceList,
    WorkspaceGet {
        workspace_id: uuid::Uuid,
    },
    WorkspaceCreate {
        request: WorkspaceCreateRequest,
    },
    WorkspaceUpdate {
        request: WorkspaceUpdateRequest,
    },
    WorkspaceActivate {
        workspace_id: uuid::Uuid,
    },
    WorkspaceDelete {
        workspace_id: uuid::Uuid,
        expected_revision: u64,
    },
    /// Capability-authenticated, account-bound Jobs handoff. The daemon
    /// validates and applies context plus profile as one session transaction.
    JobsHandoffImport {
        authorization: crate::JobsHandoffImportAuthorization,
    },
    MemorySearch {
        query: String,
        limit: usize,
    },
    AudioStatus,
    /// Explicit local-only readiness probe. The daemon uses a fixed bounded
    /// duration and never invokes STT, cloud sync, or meeting persistence.
    AudioReadinessProbe,
    AudioStart {
        enable_system: bool,
        enable_microphone: bool,
        #[serde(default)]
        mic_device_id: Option<String>,
    },
    AudioStop,
    AiStatus,
    CloudStatus,
    CloudLogin,
    CloudLogout,
    SessionsMoveLocalToCurrentAccount {
        confirmed: bool,
    },
    CloudSyncNow,
    Recap,
    ActionItems,
}

impl DaemonRequest {
    pub fn with_trace_id(self, trace_id: impl Into<String>) -> Self {
        let trace_id = trace_id.into();
        if sanitize_observability_id(&trace_id).is_none() {
            return self;
        }
        Self::WithTrace {
            trace_id,
            request: Box::new(self),
        }
    }

    pub fn into_trace_parts(self) -> (Self, Option<String>) {
        match self {
            Self::WithTrace { trace_id, request } => {
                let (request, inner_trace_id) = request.into_trace_parts();
                let trace_id = sanitize_observability_id(&trace_id).or(inner_trace_id);
                (request, trace_id)
            }
            request => (request, None),
        }
    }

    pub fn is_shutdown(&self) -> bool {
        match self {
            Self::Shutdown => true,
            Self::WithTrace { request, .. } => request.is_shutdown(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    Ok,
    Pong,
    Status {
        state: DaemonState,
    },
    Text {
        text: String,
    },
    Recap {
        recap: MeetingRecap,
    },
    ActionItems {
        items: Vec<ActionItem>,
    },
    ContextItems {
        items: Vec<ContextArtifact>,
    },
    ScreenshotContextDestination {
        owner_account_id: Option<String>,
        session_id: Option<uuid::Uuid>,
    },
    ScreenshotContextAttached {
        receipt: ScreenshotContextAttachReceipt,
    },
    AssistantProfile {
        profile: AssistantProfile,
    },
    WorkspaceList {
        workspaces: Vec<WorkspaceRecord>,
        active_workspace_id: Option<uuid::Uuid>,
    },
    Workspace {
        workspace: WorkspaceRecord,
        active_workspace_id: Option<uuid::Uuid>,
    },
    WorkspaceDeleted {
        workspace_id: uuid::Uuid,
        deleted: bool,
        active_workspace_id: Option<uuid::Uuid>,
    },
    JobsHandoffImported {
        receipt: crate::JobsHandoffImportReceipt,
    },
    MemoryHits {
        hits: Vec<MemoryHit>,
    },
    AudioStatus {
        status: AudioPipelineStatus,
    },
    AudioReadiness {
        result: AudioReadinessProbeResult,
    },
    AiStatus {
        status: AiRuntimeStatus,
    },
    Answer {
        response: AnswerResponse,
        events: Vec<AnswerStreamEvent>,
    },
    CloudStatus {
        status: CloudSyncStatus,
    },
    IpcAuthError {
        code: crate::ipc_auth::IpcAuthErrorCode,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_trace_round_trips_and_unwraps() {
        let request = DaemonRequest::Status.with_trace_id("trace-123");
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(json.contains("\"type\":\"with_trace\""));
        assert!(json.contains("\"trace_id\":\"trace-123\""));

        let decoded: DaemonRequest = serde_json::from_str(&json).expect("decode");
        let (inner, trace_id) = decoded.into_trace_parts();
        assert_eq!(trace_id.as_deref(), Some("trace-123"));
        assert!(matches!(inner, DaemonRequest::Status));
    }

    #[test]
    fn invalid_trace_wrapper_falls_back_to_inner_trace() {
        let request = DaemonRequest::WithTrace {
            trace_id: "bad\ntrace".to_string(),
            request: Box::new(DaemonRequest::Ping.with_trace_id("inner.trace")),
        };
        let (inner, trace_id) = request.into_trace_parts();
        assert_eq!(trace_id.as_deref(), Some("inner.trace"));
        assert!(matches!(inner, DaemonRequest::Ping));
    }

    #[test]
    fn shutdown_is_detected_inside_trace_envelope() {
        assert!(DaemonRequest::Shutdown.with_trace_id("trace").is_shutdown());
    }

    #[test]
    fn audio_readiness_request_round_trips_without_user_controlled_duration() {
        let value = serde_json::to_value(DaemonRequest::AudioReadinessProbe)
            .expect("serialize readiness request");
        assert_eq!(value["type"], "audio_readiness_probe");
        assert_eq!(value.as_object().map(serde_json::Map::len), Some(1));
        let decoded: DaemonRequest = serde_json::from_value(value).expect("decode readiness");
        assert!(matches!(decoded, DaemonRequest::AudioReadinessProbe));
    }

    #[test]
    fn workspace_update_contract_round_trips_with_explicit_instruction_action() {
        let workspace_id = uuid::Uuid::new_v4();
        let request = DaemonRequest::WorkspaceUpdate {
            request: WorkspaceUpdateRequest {
                workspace_id,
                expected_revision: 7,
                title: Some("Interview prep".to_string()),
                profile: None,
                instructions: crate::WorkspaceInstructionsPatch::Set {
                    text: "Use STAR examples.".to_string(),
                },
            },
        };

        let json = serde_json::to_value(&request).expect("serialize workspace update");
        assert_eq!(json["type"], "workspace_update");
        assert_eq!(json["request"]["workspace_id"], workspace_id.to_string());
        assert_eq!(json["request"]["expected_revision"], 7);
        assert_eq!(json["request"]["instructions"]["action"], "set");
        assert_eq!(
            json["request"]["instructions"]["text"],
            "Use STAR examples."
        );
        let decoded: DaemonRequest = serde_json::from_value(json).expect("decode workspace update");
        assert!(matches!(decoded, DaemonRequest::WorkspaceUpdate { .. }));
    }

    #[test]
    fn workspace_delete_contract_requires_expected_revision() {
        let workspace_id = uuid::Uuid::new_v4();
        let request = DaemonRequest::WorkspaceDelete {
            workspace_id,
            expected_revision: 9,
        };
        let json = serde_json::to_value(request).expect("serialize workspace delete");
        assert_eq!(json["type"], "workspace_delete");
        assert_eq!(json["workspace_id"], workspace_id.to_string());
        assert_eq!(json["expected_revision"], 9);
        assert!(serde_json::from_value::<DaemonRequest>(serde_json::json!({
            "type": "workspace_delete",
            "workspace_id": workspace_id,
        }))
        .is_err());
    }
}
