use serde::{Deserialize, Serialize};

use crate::{
    sanitize_observability_id, ActionItem, AiRuntimeStatus, AnswerRequest, AnswerResponse,
    AnswerStreamEvent, AudioPipelineStatus, CloudSyncStatus, ContextArtifact, CueCard, DaemonState,
    MeetingRecap, MemoryHit, OverlayPosition, Speaker,
};

pub const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:57321";

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
}
