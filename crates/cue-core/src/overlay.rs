use serde::{Deserialize, Serialize};

use crate::agent_ui::{AgentConnectorInfo, AgentSessionSummary, AgentSummary};
use crate::{overlay_ipc::ListeningState, CueCard, CueCardArtifact};

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
        Self::Center
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
    ListeningStateChanged {
        state: ListeningState,
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
    /// Push a review-gated Fix proposal for the user to approve or reject
    /// (Fix-button slice F3). The overlay renders the three sections plus the
    /// optional diff and shows Approve/Reject. `proposal_id` is the id the
    /// overlay must echo back in [`OverlayEvent::FixApprovalResponded`] — the
    /// daemon only applies a fix whose id matches a still-pending proposal, so a
    /// stale or unknown id can never trigger an apply. `apply_supported` is
    /// `false` for agents that cannot be driven to apply (e.g. no CLI); the UI
    /// disables Approve in that case.
    PushFixProposal {
        proposal_id: uuid::Uuid,
        diagnosis: String,
        reasoning: String,
        fix: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
        apply_supported: bool,
    },
    /// Push a BYOT (bring-your-own-token) billing-disclosure modal the user
    /// MUST acknowledge before a cloud agent is marked attached. Emitted by
    /// the daemon the first time the user attaches an agent whose registry
    /// row's `billing_model` is BYOT/`api_credits` and whose `vendor_short`
    /// is NOT yet in `accepted_byot_vendors` in settings.
    ///
    /// The UI renders `vendor` (display name), `billing_model` (e.g.
    /// `"api_credits"`), and the row's full `consent_warning` (`disclosure`),
    /// plus a Console URL the user can open to manage / revoke their key.
    /// Acknowledgement is the `OverlayEvent::BillingDisclosureResponded`
    /// event carrying the same `vendor_short`; the daemon refuses to attach
    /// the agent until that event arrives, so the modal is unbypassable.
    ///
    /// Data-driven by design: every field comes off the cloud-registry row
    /// (`crate::cloud::registry::CloudAgentEntry`). Adding a new BYOT vendor
    /// = adding a row, not changing the disclosure code path.
    PushBillingDisclosure {
        /// Lowercase vendor short id (matches `vendor_short` in the cloud
        /// registry row — e.g. `"anthropic"`, `"codex_cloud"`). The UI
        /// echoes this back verbatim in the response event so the daemon
        /// can match the consent to the right vendor.
        vendor_short: String,
        /// Human-facing display name (the registry row's `display_name`).
        vendor_display_name: String,
        /// Billing model label off [`crate::cloud::registry::BillingModel`]
        /// (snake_case wire form — e.g. `"api_credits"`, `"subscription"`,
        /// `"byot"`).
        billing_model: String,
        /// Verbatim disclosure copy from the registry row's
        /// `consent_warning`. The UI MUST render this in full — it's the
        /// legal disclosure (BYOT billing, ZDR ineligibility, …).
        disclosure: String,
        /// Pending agent attach to resume once the user accepts. The daemon
        /// keeps the user's original `kind` + `session_id` here so accepting
        /// the disclosure picks up the in-flight attach without a second
        /// user gesture.
        pending_kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pending_session_id: Option<String>,
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
    RemoveContextRequested {
        id: uuid::Uuid,
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
    /// UI clicked **Fix** on an agent answer (Fix-button slice F3). `question`
    /// is the problem to fix (the answer/diagnosis text); `card_id` optionally
    /// references the source card the Fix was launched from. The daemon drives
    /// the attached agent in propose-only mode and replies with a
    /// [`OverlayCommand::PushFixProposal`]; nothing is applied at this step.
    FixRequested {
        #[serde(default)]
        card_id: Option<uuid::Uuid>,
        question: String,
    },
    /// UI approved or rejected a pending Fix proposal. Carries the
    /// `proposal_id` from the [`OverlayCommand::PushFixProposal`] it is
    /// answering, so the daemon can id-match it against the still-pending
    /// proposal (a stale, replayed, or unknown id is rejected and never
    /// applied). Only `approved = true` against a live id drives an apply.
    FixApprovalResponded {
        proposal_id: uuid::Uuid,
        approved: bool,
    },
    /// UI responded to a BYOT billing disclosure modal pushed by
    /// [`OverlayCommand::PushBillingDisclosure`]. `accepted = true` means the
    /// user agreed; the daemon adds `vendor_short` to
    /// `accepted_byot_vendors` in settings and resumes the pending attach
    /// (using the `pending_kind` / `pending_session_id` echoed back here).
    /// `accepted = false` means the user declined; the daemon discards the
    /// pending attach and pushes no overlay change.
    BillingDisclosureResponded {
        /// The same `vendor_short` the push carried. The daemon uses this to
        /// (a) confirm the response matches a pending disclosure, and
        /// (b) record acknowledgement in settings.
        vendor_short: String,
        accepted: bool,
        /// Echoed from the original push so the daemon can resume the same
        /// in-flight attach.
        pending_kind: String,
        #[serde(default)]
        pending_session_id: Option<String>,
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
    SessionDeleteRequested {
        id: uuid::Uuid,
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
    fn listening_state_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::ListeningStateChanged {
            state: ListeningState::Listening,
        })
        .expect("serialize overlay listening state command");

        assert_eq!(
            json,
            r#"{"type":"listening_state_changed","state":"listening"}"#
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
                project: None,
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
    fn push_fix_proposal_serializes_with_type_tag_and_skips_absent_diff() {
        let id = uuid::Uuid::nil();
        let command = OverlayCommand::PushFixProposal {
            proposal_id: id,
            diagnosis: "PORT is read before the env var is set".to_string(),
            reasoning: "Reading it lazily fixes the ordering".to_string(),
            fix: "cargo fmt".to_string(),
            diff: None,
            apply_supported: true,
        };
        let json = serde_json::to_string(&command).expect("serialize push_fix_proposal");
        assert!(json.contains(r#""type":"push_fix_proposal""#));
        assert!(json.contains(r#""proposal_id":"00000000-0000-0000-0000-000000000000""#));
        assert!(json.contains(r#""apply_supported":true"#));
        // Absent diff is omitted from the wire form.
        assert!(!json.contains("diff"));
    }

    #[test]
    fn push_fix_proposal_includes_diff_when_present() {
        let command = OverlayCommand::PushFixProposal {
            proposal_id: uuid::Uuid::nil(),
            diagnosis: "d".to_string(),
            reasoning: "r".to_string(),
            fix: "f".to_string(),
            diff: Some("--- a\n+++ b".to_string()),
            apply_supported: false,
        };
        let json = serde_json::to_string(&command).expect("serialize");
        assert!(json.contains(r#""diff":"--- a\n+++ b""#));
        assert!(json.contains(r#""apply_supported":false"#));
    }

    #[test]
    fn fix_requested_event_deserializes_with_optional_card_id() {
        // card_id omitted -> None.
        let json = r#"{"type":"fix_requested","question":"the build fails"}"#;
        let event: OverlayEvent = serde_json::from_str(json).expect("deserialize fix_requested");
        match event {
            OverlayEvent::FixRequested { card_id, question } => {
                assert_eq!(card_id, None);
                assert_eq!(question, "the build fails");
            }
            other => panic!("expected fix_requested, got {other:?}"),
        }

        // card_id present -> Some.
        let with_card = r#"{"type":"fix_requested","card_id":"00000000-0000-0000-0000-000000000000","question":"x"}"#;
        let event: OverlayEvent = serde_json::from_str(with_card).expect("deserialize");
        match event {
            OverlayEvent::FixRequested { card_id, question } => {
                assert_eq!(card_id, Some(uuid::Uuid::nil()));
                assert_eq!(question, "x");
            }
            other => panic!("expected fix_requested, got {other:?}"),
        }
    }

    #[test]
    fn fix_approval_responded_event_roundtrips() {
        let json = r#"{"type":"fix_approval_responded","proposal_id":"00000000-0000-0000-0000-000000000000","approved":true}"#;
        let event: OverlayEvent = serde_json::from_str(json).expect("deserialize approval");
        match event {
            OverlayEvent::FixApprovalResponded {
                proposal_id,
                approved,
            } => {
                assert_eq!(proposal_id, uuid::Uuid::nil());
                assert!(approved);
            }
            other => panic!("expected fix_approval_responded, got {other:?}"),
        }

        // Re-serialize the rejection form and confirm the tag + fields.
        let reject = serde_json::to_string(&OverlayEvent::FixApprovalResponded {
            proposal_id: uuid::Uuid::nil(),
            approved: false,
        })
        .expect("serialize");
        assert!(reject.contains(r#""type":"fix_approval_responded""#));
        assert!(reject.contains(r#""approved":false"#));
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

    #[test]
    fn session_delete_event_serializes() {
        let json = serde_json::to_string(&OverlayEvent::SessionDeleteRequested {
            id: uuid::Uuid::nil(),
        })
        .expect("serialize session delete event");

        assert_eq!(
            json,
            r#"{"type":"session_delete_requested","id":"00000000-0000-0000-0000-000000000000"}"#
        );
    }
}
