use serde::{Deserialize, Serialize};

use crate::{
    sanitize_observability_id, ActionItem, AgentConnectorInfo, AgentSessionSummary, AgentSummary,
    AiRuntimeStatus, AnswerRequest, AnswerResponse, AnswerStreamEvent, AudioPipelineStatus,
    CloudSyncStatus, ContextArtifact, CueCard, DaemonState, MeetingRecap, MemoryHit,
    OverlayPosition, Speaker,
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
    /// Open the WARM meeting-backend session (the no-push pivot): rotate the
    /// MCP memory token, register Bluey's server into the attached agent, and
    /// run the warm-up drive that becomes THE session in-meeting asks resume.
    /// Fired by the calendar trigger; callable headless for tests.
    WarmupStart {
        title: Option<String>,
    },
    /// Close the warm backend: deregister Bluey's server from the agent and
    /// burn the meeting's MCP token.
    WarmupStop,
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
    CloudSyncNow,
    Recap,
    ActionItems,
    /// Discover installed coding agents and summarize each (capability,
    /// connectors, sessions, attach state).
    AgentList,
    /// Attach `kind` as the active answer-routing agent. `session_id` optionally
    /// pins a prior session to resume (validated in, persisted later). `model`
    /// optionally pins a per-run model override (applied via the agent's
    /// `model_flag`; a no-op for agents with none). Both default to `None`, so an
    /// old client that omits them still attaches with no override — back-compat.
    AgentAttach {
        kind: String,
        #[serde(default)]
        session_id: Option<String>,
        #[serde(default)]
        model: Option<String>,
    },
    /// Clear the active agent; answers fall back to Bluey's normal providers.
    AgentDetach,
    /// List one agent's prior sessions (gated on session-history consent).
    AgentSessions {
        kind: String,
    },
    /// List one agent's inherited MCP connectors (shape + readiness only).
    AgentConnectors {
        kind: String,
    },
    /// List one agent's available models (for the model picker). Not consent-
    /// gated — a model list is public. `models[0]` is always the `"auto"`
    /// sentinel.
    AgentModels {
        kind: String,
    },
    /// Set the **session-history consent** flag in the daemon's settings. Reading
    /// a coding agent's prior sessions requires explicit opt-in; this is the only
    /// writer over IPC (the dashboard's local SQLite settings do NOT reach the
    /// daemon). The UI's consent toggle routes here.
    SetAgentSessionHistory {
        enabled: bool,
    },
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
    Agents {
        agents: Vec<AgentSummary>,
    },
    AgentSessions {
        sessions: Vec<AgentSessionSummary>,
    },
    AgentConnectors {
        connectors: Vec<AgentConnectorInfo>,
    },
    AgentModels {
        models: Vec<String>,
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
    fn agent_request_variants_round_trip() {
        let cases = [
            DaemonRequest::AgentList,
            DaemonRequest::AgentAttach {
                kind: "claude_code".to_string(),
                session_id: Some("abc".to_string()),
                model: Some("composer-2.5".to_string()),
            },
            DaemonRequest::AgentDetach,
            DaemonRequest::AgentSessions {
                kind: "cursor".to_string(),
            },
            DaemonRequest::AgentConnectors {
                kind: "codex".to_string(),
            },
            DaemonRequest::AgentModels {
                kind: "cursor".to_string(),
            },
        ];
        for request in cases {
            let json = serde_json::to_string(&request).expect("serialize request");
            let decoded: DaemonRequest = serde_json::from_str(&json).expect("decode request");
            assert_eq!(
                serde_json::to_string(&decoded).expect("re-serialize"),
                json,
                "request variant should round-trip"
            );
        }
    }

    #[test]
    fn agent_attach_session_id_defaults_when_absent() {
        let json = r#"{"type":"agent_attach","kind":"claude_code"}"#;
        let decoded: DaemonRequest = serde_json::from_str(json).expect("decode");
        match decoded {
            DaemonRequest::AgentAttach {
                kind,
                session_id,
                model,
            } => {
                assert_eq!(kind, "claude_code");
                assert_eq!(session_id, None);
                // Legacy payload without "model" must default to None (back-compat).
                assert_eq!(model, None);
            }
            other => panic!("expected agent_attach, got {other:?}"),
        }
    }

    #[test]
    fn agent_response_variants_round_trip() {
        let agents = DaemonResponse::Agents {
            agents: vec![AgentSummary {
                kind: "claude_code".to_string(),
                display_name: "Claude Code".to_string(),
                capability: "drive".to_string(),
                connector_count: 2,
                ready_connector_count: 1,
                session_count: Some(4),
                attached: true,
            }],
        };
        let sessions = DaemonResponse::AgentSessions {
            sessions: vec![AgentSessionSummary {
                id: "s1".to_string(),
                title: Some("Prep".to_string()),
                updated_at: "1717000000".to_string(),
                project: None,
            }],
        };
        let connectors = DaemonResponse::AgentConnectors {
            connectors: vec![AgentConnectorInfo {
                name: "filesystem".to_string(),
                auth_tier: "env_auth".to_string(),
                ready: true,
            }],
        };
        let models = DaemonResponse::AgentModels {
            models: vec!["auto".to_string(), "composer-2.5".to_string()],
        };
        for response in [agents, sessions, connectors, models] {
            let json = serde_json::to_string(&response).expect("serialize response");
            let decoded: DaemonResponse = serde_json::from_str(&json).expect("decode response");
            assert_eq!(
                serde_json::to_string(&decoded).expect("re-serialize"),
                json,
                "response variant should round-trip"
            );
        }
    }
}
