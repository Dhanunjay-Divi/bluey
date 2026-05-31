use serde::{Deserialize, Serialize};

use crate::agent_ui::{AgentConnectorInfo, AgentSessionSummary, AgentSummary};
use crate::{CueCard, CueCardArtifact};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Default for OverlayPosition {
    fn default() -> Self {
        Self::TopRight
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlayContextItem {
    pub id: uuid::Uuid,
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverlaySessionItem {
    pub id: uuid::Uuid,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayCommand {
    Ping,
    Show,
    Hide,
    Toggle,
    Clear,
    Boot {
        title: String,
        lines: Vec<String>,
    },
    SetOpacity {
        opacity: f32,
    },
    SetPosition {
        position: OverlayPosition,
    },
    SetBalance {
        label: String,
    },
    SetContextItems {
        items: Vec<OverlayContextItem>,
    },
    SetSessions {
        sessions: Vec<OverlaySessionItem>,
    },
    PushCard {
        card: CueCard,
    },
    UpdateCard {
        id: uuid::Uuid,
        body: String,
        #[serde(default)]
        done: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact: Option<CueCardArtifact>,
    },
    /// Push the discovered-agent list to the UI (agent-bridge Slice 5a).
    SetAgents {
        agents: Vec<AgentSummary>,
    },
    /// Push one agent's prior sessions to the UI (gated on consent upstream).
    SetAgentSessions {
        kind: String,
        sessions: Vec<AgentSessionSummary>,
    },
    /// Push one agent's inherited MCP connectors (shape + readiness only).
    SetAgentConnectors {
        kind: String,
        connectors: Vec<AgentConnectorInfo>,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayEvent {
    Ready {
        platform: String,
        capture_excluded: bool,
    },
    Pong,
    Shown,
    Hidden,
    AskRequested {
        question: String,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        mode: Option<String>,
    },
    AttachRequested,
    AttachFilesRequested {
        paths: Vec<String>,
    },
    /// UI asked for the current discovered-agent list (agent-bridge Slice 5a).
    AgentListRequested,
    /// UI attached an agent; `session_id` is an optional session to resume.
    AgentAttachRequested {
        kind: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    /// UI detached the currently attached agent.
    AgentDetachRequested,
    /// UI asked for one agent's prior sessions (gated on consent in the daemon).
    AgentSessionsRequested {
        kind: String,
    },
    /// UI asked for one agent's inherited MCP connectors.
    AgentConnectorsRequested {
        kind: String,
    },
    /// UI asked to re-authenticate one hosted-OAuth connector. For now this
    /// only logs and re-emits guidance; the real OAuth flow is future work.
    ConnectorReauthRequested {
        kind: String,
        name: String,
    },
    InstructionsRequested,
    InstructionsUpdated {
        text: String,
    },
    SessionOpenRequested {
        id: uuid::Uuid,
    },
    SessionRenameRequested {
        id: uuid::Uuid,
        title: String,
    },
    SessionContinueRequested,
    SessionNewRequested,
    ActivePageCaptureRequested,
    AnalyzeScreenRequested,
    RecapRequested,
    ContextListRequested,
    CaptureStartRequested,
    CaptureStopRequested,
    RecordingStartRequested,
    RecordingStopRequested,
    CloseRequested,
    CardRendered {
        id: uuid::Uuid,
    },
    Error {
        message: String,
    },
    Lifecycle {
        stage: String,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        detail: Option<String>,
    },
    Exited,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_balance_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::SetBalance {
            label: "$12.34".to_string(),
        })
        .expect("serialize overlay balance command");

        assert_eq!(json, r#"{"type":"set_balance","label":"$12.34"}"#);
    }

    #[test]
    fn set_context_items_serializes_as_overlay_command() {
        let id = uuid::Uuid::nil();
        let json = serde_json::to_string(&OverlayCommand::SetContextItems {
            items: vec![OverlayContextItem {
                id,
                title: "GenAI Engineer JD.pdf".to_string(),
                kind: "document".to_string(),
                path: Some("/tmp/GenAI Engineer JD.pdf".to_string()),
            }],
        })
        .expect("serialize overlay context command");

        assert_eq!(
            json,
            r#"{"type":"set_context_items","items":[{"id":"00000000-0000-0000-0000-000000000000","title":"GenAI Engineer JD.pdf","kind":"document","path":"/tmp/GenAI Engineer JD.pdf"}]}"#
        );
    }

    #[test]
    fn set_sessions_serializes_as_overlay_command() {
        let id = uuid::Uuid::nil();
        let json = serde_json::to_string(&OverlayCommand::SetSessions {
            sessions: vec![OverlaySessionItem {
                id,
                title: "System design prep".to_string(),
                subtitle: "3 transcripts · 2 files".to_string(),
                is_active: true,
            }],
        })
        .expect("serialize overlay sessions command");

        assert_eq!(
            json,
            r#"{"type":"set_sessions","sessions":[{"id":"00000000-0000-0000-0000-000000000000","title":"System design prep","subtitle":"3 transcripts · 2 files","is_active":true}]}"#
        );
    }

    #[test]
    fn set_agents_serializes_with_type_tag() {
        let json = serde_json::to_string(&OverlayCommand::SetAgents {
            agents: vec![crate::agent_ui::AgentSummary {
                kind: "claude_code".to_string(),
                display_name: "Claude Code".to_string(),
                capability: "drive".to_string(),
                connector_count: 2,
                ready_connector_count: 1,
                session_count: Some(4),
                attached: true,
            }],
        })
        .expect("serialize set_agents command");

        assert!(json.starts_with(r#"{"type":"set_agents","agents":[{"#));
        assert!(json.contains(r#""kind":"claude_code""#));
    }

    #[test]
    fn set_agent_sessions_roundtrips() {
        let command = OverlayCommand::SetAgentSessions {
            kind: "cursor".to_string(),
            sessions: vec![crate::agent_ui::AgentSessionSummary {
                id: "s1".to_string(),
                title: Some("Refactor".to_string()),
                updated_at: "1717000000".to_string(),
            }],
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""type":"set_agent_sessions""#));
        assert!(json.contains(r#""kind":"cursor""#));
    }

    #[test]
    fn set_agent_connectors_roundtrips() {
        let command = OverlayCommand::SetAgentConnectors {
            kind: "codex".to_string(),
            connectors: vec![crate::agent_ui::AgentConnectorInfo {
                name: "fs".to_string(),
                auth_tier: "none".to_string(),
                ready: true,
            }],
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""type":"set_agent_connectors""#));
        assert!(json.contains(r#""name":"fs""#));
    }

    #[test]
    fn agent_attach_event_deserializes_with_optional_session() {
        let json = r#"{"type":"agent_attach_requested","kind":"claude_code"}"#;
        let event: OverlayEvent = serde_json::from_str(json).expect("deserialize attach");
        match event {
            OverlayEvent::AgentAttachRequested { kind, session_id } => {
                assert_eq!(kind, "claude_code");
                assert_eq!(session_id, None);
            }
            other => panic!("expected agent_attach_requested, got {other:?}"),
        }

        let with_session = r#"{"type":"agent_attach_requested","kind":"cursor","session_id":"s9"}"#;
        let event: OverlayEvent = serde_json::from_str(with_session).expect("deserialize");
        match event {
            OverlayEvent::AgentAttachRequested { kind, session_id } => {
                assert_eq!(kind, "cursor");
                assert_eq!(session_id.as_deref(), Some("s9"));
            }
            other => panic!("expected agent_attach_requested, got {other:?}"),
        }
    }

    #[test]
    fn agent_request_events_serialize_with_type_tag() {
        let list = serde_json::to_string(&OverlayEvent::AgentListRequested).expect("serialize");
        assert_eq!(list, r#"{"type":"agent_list_requested"}"#);

        let detach = serde_json::to_string(&OverlayEvent::AgentDetachRequested).expect("serialize");
        assert_eq!(detach, r#"{"type":"agent_detach_requested"}"#);

        let sessions = serde_json::to_string(&OverlayEvent::AgentSessionsRequested {
            kind: "gemini".to_string(),
        })
        .expect("serialize");
        assert_eq!(
            sessions,
            r#"{"type":"agent_sessions_requested","kind":"gemini"}"#
        );

        let reauth = serde_json::to_string(&OverlayEvent::ConnectorReauthRequested {
            kind: "cursor".to_string(),
            name: "remote".to_string(),
        })
        .expect("serialize");
        assert_eq!(
            reauth,
            r#"{"type":"connector_reauth_requested","kind":"cursor","name":"remote"}"#
        );
    }

    #[test]
    fn overlay_lifecycle_event_serializes() {
        let json = serde_json::to_string(&OverlayEvent::Lifecycle {
            stage: "started".to_string(),
            status: Some("ok".to_string()),
            detail: Some("capture_excluded=true".to_string()),
        })
        .expect("serialize lifecycle event");

        assert_eq!(
            json,
            r#"{"type":"lifecycle","stage":"started","status":"ok","detail":"capture_excluded=true"}"#
        );
    }
}
