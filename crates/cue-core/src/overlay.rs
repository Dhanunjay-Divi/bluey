use serde::{Deserialize, Serialize};

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

fn is_zero_usize(value: &usize) -> bool {
    *value == 0
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
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub context_count: usize,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub image_count: usize,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account_email: Option<String>,
    },
    SetAccountState {
        signed_in: bool,
    },
    SetContextItems {
        items: Vec<OverlayContextItem>,
    },
    SetSessions {
        sessions: Vec<OverlaySessionItem>,
    },
    SetActiveSession {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<uuid::Uuid>,
        code: String,
        title: String,
    },
    ListeningStateChanged {
        state: ListeningState,
    },
    TranscriptPartial {
        source: String,
        text: String,
    },
    TranscriptFinal {
        source: String,
        text: String,
    },
    SetPassthrough {
        enabled: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
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
    OpacityUpdated {
        opacity: f32,
    },
    AskRequested {
        question: String,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        visible_context_ids: Vec<uuid::Uuid>,
    },
    AttachRequested,
    AttachFilesRequested {
        paths: Vec<String>,
    },
    RemoveContextRequested {
        id: uuid::Uuid,
    },
    InstructionsRequested,
    InstructionsUpdated {
        text: String,
    },
    PasteTextRequested {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_bundle_id: Option<String>,
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
    SessionListRequested,
    SessionContinueRequested,
    SessionNewRequested,
    ActivePageCaptureRequested,
    AnalyzeScreenRequested {
        #[serde(default)]
        question: Option<String>,
    },
    RecapRequested,
    ContextListRequested,
    CaptureStartRequested,
    CaptureStopRequested,
    RecordingStartRequested,
    RecordingStopRequested,
    TranscriptClearRequested,
    SignInRequested,
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
            account_email: None,
        })
        .expect("serialize overlay balance command");

        assert_eq!(json, r#"{"type":"set_balance","label":"$12.34"}"#);
    }

    #[test]
    fn set_account_state_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::SetAccountState { signed_in: true })
            .expect("serialize overlay account-state command");

        assert_eq!(json, r#"{"type":"set_account_state","signed_in":true}"#);
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
                context_count: 2,
                image_count: 1,
                is_active: true,
            }],
        })
        .expect("serialize overlay sessions command");

        assert_eq!(
            json,
            r#"{"type":"set_sessions","sessions":[{"id":"00000000-0000-0000-0000-000000000000","title":"System design prep","subtitle":"3 transcripts · 2 files","context_count":2,"image_count":1,"is_active":true}]}"#
        );
    }

    #[test]
    fn set_active_session_serializes_as_overlay_command() {
        let id = uuid::Uuid::nil();
        let json = serde_json::to_string(&OverlayCommand::SetActiveSession {
            id: Some(id),
            code: "00000000".to_string(),
            title: "New recording".to_string(),
        })
        .expect("serialize active session command");

        assert_eq!(
            json,
            r#"{"type":"set_active_session","id":"00000000-0000-0000-0000-000000000000","code":"00000000","title":"New recording"}"#
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
    fn set_passthrough_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::SetPassthrough {
            enabled: true,
            duration_ms: Some(900),
        })
        .expect("serialize overlay passthrough command");

        assert_eq!(
            json,
            r#"{"type":"set_passthrough","enabled":true,"duration_ms":900}"#
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

    #[test]
    fn session_list_event_serializes() {
        let json = serde_json::to_string(&OverlayEvent::SessionListRequested)
            .expect("serialize session list event");

        assert_eq!(json, r#"{"type":"session_list_requested"}"#);
    }

    #[test]
    fn paste_text_event_serializes_with_target_bundle() {
        let json = serde_json::to_string(&OverlayEvent::PasteTextRequested {
            text: "hello".to_string(),
            target_bundle_id: Some("com.apple.TextEdit".to_string()),
        })
        .expect("serialize paste text event");

        assert_eq!(
            json,
            r#"{"type":"paste_text_requested","text":"hello","target_bundle_id":"com.apple.TextEdit"}"#
        );
    }

    #[test]
    fn sign_in_event_serializes() {
        let json =
            serde_json::to_string(&OverlayEvent::SignInRequested).expect("serialize sign-in event");

        assert_eq!(json, r#"{"type":"sign_in_requested"}"#);
    }
}
