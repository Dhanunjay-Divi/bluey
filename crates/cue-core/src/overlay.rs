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
    /// The project/workspace this session belongs to, when known (used for the
    /// redesigned panel's project filter chip). `None` when unassociated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Best-effort last-updated marker (epoch seconds or RFC3339 string, per
    /// source). Drives the date-group bucketing (Today / Yesterday / …). Empty
    /// string when unknown (older payloads).
    #[serde(default)]
    pub updated_at: String,
    /// Turn/exchange count, when cheaply countable (shown as "N turns").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_count: Option<usize>,
    /// True when the user has pinned this session to the top of the list.
    #[serde(default)]
    pub pinned: bool,
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
        /// Conversation turn count for the active meeting, shown as the overlay's
        /// "In context: N turns" indicator. `0` when no active meeting / no turns.
        #[serde(default)]
        turns: usize,
    },
    SetSessions {
        sessions: Vec<OverlaySessionItem>,
    },
    /// Paginated/searched session page — the redesigned panel's at-scale path.
    /// Unlike [`OverlayCommand::SetSessions`] (a one-shot, capped initial paint),
    /// this answers an [`OverlayEvent::SessionsRequested`] and carries the slice
    /// the UI asked for plus the totals it needs to render "show N more" and a
    /// result count. `sessions` are already sorted by the daemon (pinned first,
    /// then most-recent) and each item carries its `pinned`/`project`/`updated_at`
    /// so the UI can group by date and filter by project without another round
    /// trip. `query`/`offset` are echoed so a late/out-of-order reply can be
    /// matched to (or discarded against) the UI's current request.
    SetSessionsPage {
        sessions: Vec<OverlaySessionItem>,
        /// Total sessions matching the current `query` (before paging) — the UI
        /// uses this for "show N more" and the "Search 318 sessions…" count.
        total: usize,
        /// The offset this page starts at (echo of the request).
        offset: usize,
        /// True when `offset + sessions.len() < total` (more pages remain).
        has_more: bool,
        /// Echo of the search string this page answers (empty = unfiltered), so
        /// the UI can ignore a reply that no longer matches what's typed.
        #[serde(default)]
        query: String,
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
    /// `offset`/`limit`/`search` support the redesigned at-scale agent-session
    /// list; all default (0/0/"") to the prior "first page, unfiltered" behavior
    /// so existing callers and payloads are unaffected.
    AgentSessionsRequested {
        kind: String,
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        limit: usize,
        #[serde(default)]
        search: String,
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
    /// UI requested a page of sessions for the at-scale list (redesign). The
    /// daemon answers with [`OverlayCommand::SetSessionsPage`]: filter by
    /// `search` (title/project, case-insensitive; empty = all), sort pinned-first
    /// then most-recent, and return the `offset..offset+limit` window plus the
    /// total. This replaces the implicit cap-at-8 of [`OverlayCommand::SetSessions`].
    SessionsRequested {
        #[serde(default)]
        offset: usize,
        /// Page size. The daemon clamps to a sane max; `0` means "daemon default".
        #[serde(default)]
        limit: usize,
        /// Case-insensitive filter over title + project. Empty = unfiltered.
        #[serde(default)]
        search: String,
    },
    /// UI pinned a session to the top of the list. The daemon persists the pin
    /// and re-sends the affected page so the move is reflected.
    SessionPinRequested {
        id: uuid::Uuid,
    },
    /// UI unpinned a previously pinned session.
    SessionUnpinRequested {
        id: uuid::Uuid,
    },
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
            turns: 3,
        })
        .expect("serialize overlay context command");

        assert_eq!(
            json,
            r#"{"type":"set_context_items","items":[{"id":"00000000-0000-0000-0000-000000000000","title":"GenAI Engineer JD.pdf","kind":"document","path":"/tmp/GenAI Engineer JD.pdf"}],"turns":3}"#
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
                project: None,
                updated_at: String::new(),
                turn_count: None,
                pinned: false,
            }],
        })
        .expect("serialize overlay sessions command");

        // `project`/`turn_count` are `skip_serializing_if = None`, so an item
        // with neither only adds the always-present `updated_at` + `pinned`.
        assert_eq!(
            json,
            r#"{"type":"set_sessions","sessions":[{"id":"00000000-0000-0000-0000-000000000000","title":"System design prep","subtitle":"3 transcripts · 2 files","is_active":true,"updated_at":"","pinned":false}]}"#
        );
    }

    #[test]
    fn legacy_session_item_decodes_with_defaulted_new_fields() {
        // An old daemon (pre-redesign) sends only the original four fields. The
        // extended struct must still decode, defaulting the new ones, so a
        // version skew never drops the session list.
        let legacy = r#"{"id":"00000000-0000-0000-0000-000000000000","title":"t","subtitle":"s","is_active":false}"#;
        let item: OverlaySessionItem = serde_json::from_str(legacy).expect("decode legacy item");
        assert_eq!(item.project, None);
        assert_eq!(item.updated_at, "");
        assert_eq!(item.turn_count, None);
        assert!(!item.pinned);
    }

    #[test]
    fn set_sessions_page_round_trips_with_paging_fields() {
        let cmd = OverlayCommand::SetSessionsPage {
            sessions: vec![OverlaySessionItem {
                id: uuid::Uuid::nil(),
                title: "Overlay redesign".to_string(),
                subtitle: "9 turns".to_string(),
                is_active: true,
                project: Some("Bluey".to_string()),
                updated_at: "1718000000".to_string(),
                turn_count: Some(9),
                pinned: true,
            }],
            total: 318,
            offset: 0,
            has_more: true,
            query: "redesign".to_string(),
        };
        let json = serde_json::to_string(&cmd).expect("serialize page");
        assert!(json.contains(r#""type":"set_sessions_page""#));
        assert!(json.contains(r#""total":318"#));
        assert!(json.contains(r#""has_more":true"#));
        let decoded: OverlayCommand = serde_json::from_str(&json).expect("decode page");
        assert_eq!(
            serde_json::to_string(&decoded).expect("re-serialize"),
            json,
            "SetSessionsPage should round-trip"
        );
    }

    #[test]
    fn session_paging_and_pin_events_round_trip() {
        let cases = [
            OverlayEvent::SessionsRequested {
                offset: 20,
                limit: 20,
                search: "auth".to_string(),
            },
            OverlayEvent::SessionPinRequested {
                id: uuid::Uuid::nil(),
            },
            OverlayEvent::SessionUnpinRequested {
                id: uuid::Uuid::nil(),
            },
        ];
        for event in cases {
            let json = serde_json::to_string(&event).expect("serialize event");
            let decoded: OverlayEvent = serde_json::from_str(&json).expect("decode event");
            assert_eq!(
                serde_json::to_string(&decoded).expect("re-serialize"),
                json,
                "session paging/pin event should round-trip"
            );
        }
    }

    #[test]
    fn agent_sessions_requested_defaults_paging_when_absent() {
        // The prior wire form carried only `kind`; it must still decode with the
        // new paging fields defaulted (offset 0, limit 0, empty search).
        let legacy = r#"{"type":"agent_sessions_requested","kind":"claude_code"}"#;
        let decoded: OverlayEvent = serde_json::from_str(legacy).expect("decode legacy");
        match decoded {
            OverlayEvent::AgentSessionsRequested {
                kind,
                offset,
                limit,
                search,
            } => {
                assert_eq!(kind, "claude_code");
                assert_eq!(offset, 0);
                assert_eq!(limit, 0);
                assert_eq!(search, "");
            }
            other => panic!("expected agent_sessions_requested, got {other:?}"),
        }
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
            offset: 0,
            limit: 0,
            search: String::new(),
        })
        .expect("serialize");
        assert_eq!(
            sessions,
            r#"{"type":"agent_sessions_requested","kind":"gemini","offset":0,"limit":0,"search":""}"#
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
