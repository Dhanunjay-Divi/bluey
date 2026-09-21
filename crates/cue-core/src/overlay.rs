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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_status: Option<String>,
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

/// A single, privacy-preserving observation used to decide whether a meeting
/// is actually in progress. Native helpers report metadata only; recording
/// never starts until the daemon accepts an explicit/authorized action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingEvidence {
    /// Native source, for example `coreaudio_process` or `wasapi_session`.
    pub source: String,
    /// Human-readable host application name.
    pub app_name: String,
    /// Stable bundle ID or executable identity.
    pub app_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    #[serde(default)]
    pub audio_input_active: bool,
    #[serde(default)]
    pub audio_output_active: bool,
    #[serde(default)]
    pub app_foreground: bool,
    #[serde(default)]
    pub browser: bool,
    #[serde(default)]
    pub dedicated_meeting_app: bool,
    pub observed_at_unix_ms: i64,
}

/// A corroborated meeting candidate produced by the daemon state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingCandidate {
    pub candidate_id: String,
    pub app_name: String,
    pub app_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Normalized confidence from 0 through 100.
    pub confidence: u8,
    /// Stable, machine-readable evidence labels used for audit/debug UI.
    #[serde(default)]
    pub provenance: Vec<String>,
    /// Concise user-facing explanation of why Bluey surfaced the banner.
    pub reason: String,
    #[serde(default)]
    pub browser: bool,
    pub first_seen_unix_ms: i64,
    pub last_seen_unix_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeetingBannerAction {
    Start,
    Dismiss,
    /// The native banner reached its display timeout without user input.
    /// This is intentionally distinct from a manual dismissal so the detector
    /// applies only its short re-entry cooldown.
    Expired,
    Snooze,
    Ignore,
    Settings,
}

/// Native rendering milestone requested for an answer-card update.
///
/// These phases describe actual visible paint acknowledgements, not receipt of
/// an IPC message. Progress/status text must not request `FirstText`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnswerRenderAckPhase {
    FirstText,
    Final,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer_instructions: Option<String>,
    },
    ListeningStateChanged {
        state: ListeningState,
    },
    AudioAutoStopCountdown {
        remaining_secs: u64,
        idle_secs: u64,
    },
    AudioAutoStopCountdownCleared,
    /// Enables or fully suspends native meeting-evidence sampling. Disabling
    /// this command must also hide any pending native meeting banner.
    SetMeetingDetectionEnabled {
        enabled: bool,
    },
    ShowMeetingBanner {
        candidate: MeetingCandidate,
        #[serde(default = "default_meeting_banner_timeout_secs")]
        timeout_secs: u64,
    },
    HideMeetingBanner {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        candidate_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
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
        /// Ephemeral UUID shared by one user ask and its answer render path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interaction_id: Option<uuid::Uuid>,
        body: String,
        #[serde(default)]
        done: bool,
        /// Monotonic per-card presentation sequence. Native overlays reject
        /// stale non-snapshot updates after reconnect or scheduling jitter.
        #[serde(default)]
        sequence: u64,
        /// A snapshot may replace local presentation state after an overlay
        /// restart even when its sequence was already observed.
        #[serde(default)]
        snapshot: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact: Option<CueCardArtifact>,
        /// Rendering milestone the native overlay should acknowledge only
        /// after this update is visibly painted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        render_ack: Option<AnswerRenderAckPhase>,
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
        /// UUID minted once by the native UI for this user interaction.
        /// Legacy overlays omit it and the daemon supplies a fallback.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interaction_id: Option<uuid::Uuid>,
        /// Native wall-clock timestamp captured when the user action was
        /// accepted. It contains no user content and lets the daemon measure
        /// native dispatch/queue latency. Legacy overlays omit it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        initiated_at_unix_ms: Option<u64>,
        #[serde(default)]
        provider: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        visible_context_ids: Vec<uuid::Uuid>,
        #[serde(default)]
        answer_current_transcript: bool,
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
    MeetingEvidenceObserved {
        evidence: MeetingEvidence,
    },
    MeetingBannerAction {
        candidate_id: String,
        action: MeetingBannerAction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
    },
    TranscriptClearRequested,
    SignInRequested,
    CloseRequested,
    CardRendered {
        id: uuid::Uuid,
    },
    /// Confirms that a requested answer milestone reached the visible native
    /// surface. This remains separate from the legacy generic `CardRendered`
    /// event so existing card-installation semantics do not change.
    AnswerRenderAcknowledged {
        id: uuid::Uuid,
        interaction_id: uuid::Uuid,
        phase: AnswerRenderAckPhase,
        sequence: u64,
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

const fn default_meeting_banner_timeout_secs() -> u64 {
    12
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
                processing_status: Some("ready".to_string()),
            }],
        })
        .expect("serialize overlay context command");

        assert_eq!(
            json,
            r#"{"type":"set_context_items","items":[{"id":"00000000-0000-0000-0000-000000000000","title":"GenAI Engineer JD.pdf","kind":"document","path":"/tmp/GenAI Engineer JD.pdf","processing_status":"ready"}]}"#
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
            answer_instructions: Some("Keep answers concise.".to_string()),
        })
        .expect("serialize active session command");

        assert_eq!(
            json,
            r#"{"type":"set_active_session","id":"00000000-0000-0000-0000-000000000000","code":"00000000","title":"New recording","answer_instructions":"Keep answers concise."}"#
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
    fn audio_auto_stop_countdown_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::AudioAutoStopCountdown {
            remaining_secs: 10,
            idle_secs: 60,
        })
        .expect("serialize audio auto-stop countdown command");

        assert_eq!(
            json,
            r#"{"type":"audio_auto_stop_countdown","remaining_secs":10,"idle_secs":60}"#
        );
    }

    #[test]
    fn audio_auto_stop_countdown_clear_serializes_as_overlay_command() {
        let json = serde_json::to_string(&OverlayCommand::AudioAutoStopCountdownCleared)
            .expect("serialize audio auto-stop countdown clear command");

        assert_eq!(json, r#"{"type":"audio_auto_stop_countdown_cleared"}"#);
    }

    #[test]
    fn meeting_evidence_event_roundtrips_without_content_capture() {
        let event = OverlayEvent::MeetingEvidenceObserved {
            evidence: MeetingEvidence {
                source: "coreaudio_process".to_string(),
                app_name: "Google Chrome".to_string(),
                app_id: "com.google.Chrome".to_string(),
                process_id: Some(42),
                provider: Some("google_meet".to_string()),
                window_title: Some("Daily sync - Google Meet".to_string()),
                page_url: None,
                audio_input_active: true,
                audio_output_active: true,
                app_foreground: true,
                browser: true,
                dedicated_meeting_app: false,
                observed_at_unix_ms: 1_700_000_000_000,
            },
        };

        let json = serde_json::to_string(&event).expect("serialize meeting evidence");
        assert!(!json.contains("transcript"));
        let decoded: OverlayEvent =
            serde_json::from_str(&json).expect("deserialize meeting evidence");
        assert!(matches!(
            decoded,
            OverlayEvent::MeetingEvidenceObserved {
                evidence: MeetingEvidence {
                    browser: true,
                    audio_input_active: true,
                    ..
                }
            }
        ));
    }

    #[test]
    fn meeting_banner_command_includes_confidence_and_provenance() {
        let command = OverlayCommand::ShowMeetingBanner {
            candidate: MeetingCandidate {
                candidate_id: "com.google.Chrome:google_meet".to_string(),
                app_name: "Google Chrome".to_string(),
                app_id: "com.google.Chrome".to_string(),
                provider: Some("google_meet".to_string()),
                confidence: 92,
                provenance: vec![
                    "audio_input".to_string(),
                    "provider_window_title".to_string(),
                ],
                reason: "Chrome is using the microphone in a Google Meet window.".to_string(),
                browser: true,
                first_seen_unix_ms: 1,
                last_seen_unix_ms: 2,
            },
            timeout_secs: 12,
        };

        let json = serde_json::to_string(&command).expect("serialize meeting banner");
        assert!(json.contains(r#""type":"show_meeting_banner""#));
        assert!(json.contains(r#""confidence":92"#));
        assert!(json.contains("provider_window_title"));
    }

    #[test]
    fn meeting_banner_action_roundtrips() {
        let event = OverlayEvent::MeetingBannerAction {
            candidate_id: "us.zoom.xos".to_string(),
            action: MeetingBannerAction::Snooze,
            app_id: Some("us.zoom.xos".to_string()),
            provider: Some("zoom".to_string()),
        };
        let json = serde_json::to_string(&event).expect("serialize banner action");
        assert_eq!(
            json,
            r#"{"type":"meeting_banner_action","candidate_id":"us.zoom.xos","action":"snooze","app_id":"us.zoom.xos","provider":"zoom"}"#
        );
    }

    #[test]
    fn meeting_banner_expiration_is_not_serialized_as_manual_dismissal() {
        let event = OverlayEvent::MeetingBannerAction {
            candidate_id: "us.zoom.xos".to_string(),
            action: MeetingBannerAction::Expired,
            app_id: Some("us.zoom.xos".to_string()),
            provider: Some("zoom".to_string()),
        };
        let json = serde_json::to_string(&event).expect("serialize expired banner action");
        assert!(json.contains(r#""action":"expired""#));
        assert!(!json.contains(r#""action":"dismiss""#));
    }

    #[test]
    fn meeting_detection_switch_roundtrips() {
        let command = OverlayCommand::SetMeetingDetectionEnabled { enabled: false };
        let json = serde_json::to_string(&command).expect("serialize meeting switch");
        assert_eq!(
            json,
            r#"{"type":"set_meeting_detection_enabled","enabled":false}"#
        );
        let decoded: OverlayCommand =
            serde_json::from_str(&json).expect("deserialize meeting switch");
        assert!(matches!(
            decoded,
            OverlayCommand::SetMeetingDetectionEnabled { enabled: false }
        ));
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
    fn update_card_carries_monotonic_sequence_and_snapshot_marker() {
        let id = uuid::Uuid::nil();
        let command = OverlayCommand::UpdateCard {
            id,
            interaction_id: Some(id),
            body: "Recovered answer".to_string(),
            done: false,
            sequence: 7,
            snapshot: true,
            cost_label: None,
            artifact: None,
            render_ack: Some(AnswerRenderAckPhase::FirstText),
        };

        let json = serde_json::to_string(&command).expect("serialize update card");
        assert!(json.contains(r#""sequence":7"#));
        assert!(json.contains(r#""snapshot":true"#));
        assert!(json.contains(r#""interaction_id":"00000000-0000-0000-0000-000000000000""#));
        assert!(json.contains(r#""render_ack":"first_text""#));
        let decoded: OverlayCommand = serde_json::from_str(&json).expect("deserialize update card");
        assert!(matches!(
            decoded,
            OverlayCommand::UpdateCard {
                sequence: 7,
                snapshot: true,
                interaction_id: Some(interaction_id),
                render_ack: Some(AnswerRenderAckPhase::FirstText),
                ..
            } if interaction_id == id
        ));
    }

    #[test]
    fn legacy_update_card_defaults_interaction_fields_to_none() {
        let command: OverlayCommand = serde_json::from_str(
            r#"{"type":"update_card","id":"00000000-0000-0000-0000-000000000000","body":"Legacy","done":false}"#,
        )
        .expect("deserialize legacy update card");

        assert!(matches!(
            command,
            OverlayCommand::UpdateCard {
                interaction_id: None,
                render_ack: None,
                ..
            }
        ));
    }

    #[test]
    fn answer_render_acknowledgement_round_trips() {
        let interaction_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .expect("valid interaction id");
        let event = OverlayEvent::AnswerRenderAcknowledged {
            id: uuid::Uuid::nil(),
            interaction_id,
            phase: AnswerRenderAckPhase::Final,
            sequence: 9,
        };

        let json = serde_json::to_string(&event).expect("serialize render acknowledgement");
        assert_eq!(
            json,
            r#"{"type":"answer_render_acknowledged","id":"00000000-0000-0000-0000-000000000000","interaction_id":"550e8400-e29b-41d4-a716-446655440000","phase":"final","sequence":9}"#
        );
        let decoded: OverlayEvent =
            serde_json::from_str(&json).expect("deserialize render acknowledgement");
        assert!(matches!(
            decoded,
            OverlayEvent::AnswerRenderAcknowledged {
                interaction_id: decoded_id,
                phase: AnswerRenderAckPhase::Final,
                sequence: 9,
                ..
            } if decoded_id == interaction_id
        ));
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

    #[test]
    fn ask_event_defaults_transcript_intent_to_false() {
        let event: OverlayEvent =
            serde_json::from_str(r#"{"type":"ask_requested","question":"hello"}"#)
                .expect("deserialize legacy ask event");

        assert!(matches!(
            event,
            OverlayEvent::AskRequested {
                interaction_id: None,
                answer_current_transcript: false,
                ..
            }
        ));
    }

    #[test]
    fn ask_event_preserves_interaction_id() {
        let interaction_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
            .expect("valid interaction id");
        let event: OverlayEvent = serde_json::from_str(
            r#"{"type":"ask_requested","question":"answer this","interaction_id":"550e8400-e29b-41d4-a716-446655440000"}"#,
        )
        .expect("deserialize interaction ask event");

        assert!(matches!(
            event,
            OverlayEvent::AskRequested {
                interaction_id: Some(decoded_id),
                ..
            } if decoded_id == interaction_id
        ));
    }

    #[test]
    fn ask_event_preserves_native_initiation_timestamp() {
        let event: OverlayEvent = serde_json::from_str(
            r#"{"type":"ask_requested","question":"answer this","initiated_at_unix_ms":1750000000123}"#,
        )
        .expect("deserialize timestamped ask event");

        assert!(matches!(
            event,
            OverlayEvent::AskRequested {
                initiated_at_unix_ms: Some(1_750_000_000_123),
                ..
            }
        ));
    }

    #[test]
    fn ask_event_preserves_explicit_transcript_intent() {
        let event: OverlayEvent = serde_json::from_str(
            r#"{"type":"ask_requested","question":"answer this","answer_current_transcript":true}"#,
        )
        .expect("deserialize transcript ask event");

        assert!(matches!(
            event,
            OverlayEvent::AskRequested {
                answer_current_transcript: true,
                ..
            }
        ));
    }
}
