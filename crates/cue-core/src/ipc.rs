use serde::{Deserialize, Serialize};

use crate::{
    sanitize_observability_id, ActionItem, AiRuntimeStatus, AnswerRequest, AnswerResponse,
    AnswerStreamEvent, AudioPipelineStatus, CloudSyncStatus, ContextArtifact, CueCard, DaemonState,
    MeetingRecap, MemoryHit, OverlayPosition, Speaker,
};

pub const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:57321";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonSessionRecord {
    pub id: uuid::Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_account_id: Option<String>,
    pub title: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonSessionLifecycle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed: Option<DaemonSessionRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced: Option<DaemonSessionRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<DaemonSessionRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<uuid::Uuid>,
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
    SessionCreate {
        title: Option<String>,
    },
    SessionActivate {
        id: uuid::Uuid,
    },
    SessionContinue,
    SessionDeactivate,
    SessionRename {
        id: uuid::Uuid,
        title: String,
    },
    SessionArchive {
        id: uuid::Uuid,
    },
    SessionDelete {
        id: uuid::Uuid,
    },
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
    ContextList,
    ActivePageCapture,
    ScreenCaptureStart {
        interval_secs: Option<u64>,
    },
    ScreenCaptureStop,
    MeetingDetectionSettingsReload,
    InstructionsSet {
        text: String,
    },
    InstructionsGet,
    InstructionsClear,
    MemorySearch {
        query: String,
        limit: usize,
    },
    AudioStatus,
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
    MemoryHits {
        hits: Vec<MemoryHit>,
    },
    AudioStatus {
        status: AudioPipelineStatus,
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
    SessionLifecycle {
        lifecycle: DaemonSessionLifecycle,
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
    fn session_lifecycle_round_trips_exact_ids_and_owner() {
        let id = uuid::Uuid::new_v4();
        let lifecycle = DaemonSessionLifecycle {
            changed: Some(DaemonSessionRecord {
                id,
                owner_account_id: Some("account-a".to_string()),
                title: "Canonical".to_string(),
                started_at: "1000".to_string(),
                ended_at: None,
                active: true,
            }),
            replaced: None,
            deleted: None,
            active_session_id: Some(id),
        };
        let json = serde_json::to_string(&DaemonResponse::SessionLifecycle {
            lifecycle: lifecycle.clone(),
        })
        .expect("serialize lifecycle");
        let decoded: DaemonResponse = serde_json::from_str(&json).expect("decode lifecycle");
        match decoded {
            DaemonResponse::SessionLifecycle { lifecycle: decoded } => {
                assert_eq!(decoded, lifecycle)
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
