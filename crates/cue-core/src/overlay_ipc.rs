//! Wire protocol for daemon <-> native-overlay IPC.
//!
//! The native Swift (macOS) and C (Windows) overlays live in separate
//! processes from `cue-daemon` and communicate via JSON over stdin/stdout.
//! Both sides must agree on the message schema, so it lives here in `cue-core`
//! (shared by the Rust sender and, once the Swift/C clients are updated, by
//! the receiver via equivalent definitions).

use serde::{Deserialize, Serialize};

/// Maximum field lengths for overlay event validation (Item 6).
pub const MAX_QUESTION_LEN: usize = 4096;
pub const MAX_INSTRUCTIONS_LEN: usize = 16384;
pub const MAX_PATH_LEN: usize = 1024;
pub const MAX_PATHS_COUNT: usize = 16;
pub const MAX_ERROR_LEN: usize = 4096;
pub const MAX_TEXT_LEN: usize = 65536;

/// A message pushed from `cue-daemon` to a native overlay process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayMessage {
    SessionSwitched {
        session_id: Option<String>,
        title: Option<String>,
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
    },
    Ping,
}

/// Coarse-grained overlay-visible state of the listening pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListeningState {
    Idle,
    Connecting,
    Listening,
    Paused,
    Failed,
}

/// Messages the overlay can send BACK to the daemon (IPC).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayIpcCommand {
    Pong,
    RequestSync,
    Echo { payload: String },
    AskRequested { question: String },
    AttachFilesRequested { paths: Vec<String> },
    InstructionsUpdated { instructions: String },
}

/// Wrapper for overlay events that includes the session token for validation.
/// Every message from the overlay to the daemon is wrapped in this envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayEvent {
    /// Session token provided by the daemon at spawn time.
    /// Must match the daemon's current session token or the event is dropped.
    #[serde(default)]
    pub token: String,
    /// The actual IPC command from the overlay.
    #[serde(flatten)]
    pub command: OverlayIpcCommand,
}

/// Overlay UI state for event validation (Item 4).
/// Tracks which UI affordance is currently open so the daemon can reject
/// events that don't match the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayUiState {
    Idle,
    AttachOpen,
    InstructionsOpen,
}

impl Default for OverlayUiState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Discriminant of an `OverlayIpcCommand` for state-machine validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayEventKind {
    Pong,
    RequestSync,
    Echo,
    AskRequested,
    AttachFilesRequested,
    InstructionsUpdated,
}

impl OverlayEventKind {
    pub fn from_command(cmd: &OverlayIpcCommand) -> Self {
        match cmd {
            OverlayIpcCommand::Pong => Self::Pong,
            OverlayIpcCommand::RequestSync => Self::RequestSync,
            OverlayIpcCommand::Echo { .. } => Self::Echo,
            OverlayIpcCommand::AskRequested { .. } => Self::AskRequested,
            OverlayIpcCommand::AttachFilesRequested { .. } => Self::AttachFilesRequested,
            OverlayIpcCommand::InstructionsUpdated { .. } => Self::InstructionsUpdated,
        }
    }

    /// Returns true if this event kind is allowed in the given UI state.
    pub fn is_allowed_in(self, state: OverlayUiState) -> bool {
        match self {
            Self::Pong | Self::RequestSync | Self::Echo | Self::AskRequested => true,
            Self::AttachFilesRequested => {
                state == OverlayUiState::Idle || state == OverlayUiState::AttachOpen
            }
            Self::InstructionsUpdated => state == OverlayUiState::InstructionsOpen,
        }
    }
}

/// Validate field lengths on an `OverlayIpcCommand`. Returns an error
/// description if any field exceeds its limit.
pub fn validate_command_lengths(cmd: &OverlayIpcCommand) -> Result<(), String> {
    match cmd {
        OverlayIpcCommand::AskRequested { question } => {
            if question.len() > MAX_QUESTION_LEN {
                return Err(format!(
                    "question field exceeds max length ({} > {})",
                    question.len(),
                    MAX_QUESTION_LEN
                ));
            }
        }
        OverlayIpcCommand::AttachFilesRequested { paths } => {
            if paths.len() > MAX_PATHS_COUNT {
                return Err(format!(
                    "paths array exceeds max count ({} > {})",
                    paths.len(),
                    MAX_PATHS_COUNT
                ));
            }
            for (i, p) in paths.iter().enumerate() {
                if p.len() > MAX_PATH_LEN {
                    return Err(format!(
                        "paths[{}] exceeds max length ({} > {})",
                        i,
                        p.len(),
                        MAX_PATH_LEN
                    ));
                }
            }
        }
        OverlayIpcCommand::InstructionsUpdated { instructions } => {
            if instructions.len() > MAX_INSTRUCTIONS_LEN {
                return Err(format!(
                    "instructions field exceeds max length ({} > {})",
                    instructions.len(),
                    MAX_INSTRUCTIONS_LEN
                ));
            }
        }
        OverlayIpcCommand::Echo { payload } => {
            if payload.len() > MAX_TEXT_LEN {
                return Err(format!(
                    "echo payload exceeds max length ({} > {})",
                    payload.len(),
                    MAX_TEXT_LEN
                ));
            }
        }
        OverlayIpcCommand::Pong | OverlayIpcCommand::RequestSync => {}
    }
    Ok(())
}

/// Serialize an `OverlayMessage` to a single-line NDJSON string (newline appended).
pub fn encode_ndjson(msg: &OverlayMessage) -> Result<String, serde_json::Error> {
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    Ok(s)
}

/// Parse a single NDJSON line into an `OverlayMessage`. Enforces field length limits.
pub fn decode_ndjson(line: &str) -> Result<OverlayMessage, serde_json::Error> {
    let trimmed = line.trim_end_matches('\n');
    let msg: OverlayMessage = serde_json::from_str(trimmed)?;
    validate_message_lengths(&msg).map_err(serde::de::Error::custom)?;
    Ok(msg)
}

/// Parse a single NDJSON line into an `OverlayIpcCommand` with length validation.
pub fn decode_command_ndjson(line: &str) -> Result<OverlayIpcCommand, String> {
    let trimmed = line.trim_end_matches('\n');
    let cmd: OverlayIpcCommand =
        serde_json::from_str(trimmed).map_err(|e| format!("JSON parse error: {e}"))?;
    validate_command_lengths(&cmd)?;
    Ok(cmd)
}

#[allow(clippy::collapsible_if, clippy::collapsible_match)]
fn validate_message_lengths(msg: &OverlayMessage) -> Result<(), String> {
    match msg {
        OverlayMessage::TranscriptPartial { text, .. }
        | OverlayMessage::TranscriptFinal { text, .. } => {
            if text.len() > MAX_TEXT_LEN {
                return Err(format!(
                    "text field exceeds max length ({} > {})",
                    text.len(),
                    MAX_TEXT_LEN
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_switched_roundtrip_with_id_and_title() {
        let msg = OverlayMessage::SessionSwitched {
            session_id: Some("abc-123".into()),
            title: Some("My session".into()),
        };
        let line = encode_ndjson(&msg).unwrap();
        assert!(line.ends_with('\n'));
        let parsed = decode_ndjson(&line).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn session_switched_none_serializes_cleanly() {
        let msg = OverlayMessage::SessionSwitched {
            session_id: None,
            title: None,
        };
        let line = encode_ndjson(&msg).unwrap();
        assert!(line.contains(r#""type":"session_switched""#));
        assert!(line.contains(r#""session_id":null"#));
        let parsed = decode_ndjson(&line).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn transcript_partial_roundtrip() {
        let msg = OverlayMessage::TranscriptPartial {
            source: "microphone".into(),
            text: "hello wor".into(),
        };
        let line = encode_ndjson(&msg).unwrap();
        let parsed = decode_ndjson(&line).unwrap();
        assert_eq!(msg, parsed);
    }

    #[test]
    fn decode_accepts_line_with_trailing_newline() {
        let parsed: OverlayMessage = decode_ndjson("{\"type\":\"ping\"}\n").unwrap();
        assert_eq!(parsed, OverlayMessage::Ping);
    }

    #[test]
    fn overlay_command_roundtrip() {
        let cmd = OverlayIpcCommand::RequestSync;
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("request_sync"));
        let back: OverlayIpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, back);
    }

    #[test]
    fn listening_state_serializes_snake_case() {
        let s = serde_json::to_string(&ListeningState::Listening).unwrap();
        assert_eq!(s, r#""listening""#);
    }

    #[test]
    fn unknown_type_fails_to_decode() {
        let res = decode_ndjson(r#"{"type":"nonsense"}"#);
        assert!(res.is_err());
    }

    #[test]
    fn set_passthrough_roundtrip() {
        let msg = OverlayMessage::SetPassthrough { enabled: false };
        let line = encode_ndjson(&msg).unwrap();
        assert!(line.contains("set_passthrough"));
        let parsed = decode_ndjson(&line).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn pong_allowed_in_all_states() {
        assert!(OverlayEventKind::Pong.is_allowed_in(OverlayUiState::Idle));
        assert!(OverlayEventKind::Pong.is_allowed_in(OverlayUiState::AttachOpen));
        assert!(OverlayEventKind::Pong.is_allowed_in(OverlayUiState::InstructionsOpen));
    }

    #[test]
    fn ask_requested_allowed_in_all_states() {
        assert!(OverlayEventKind::AskRequested.is_allowed_in(OverlayUiState::Idle));
        assert!(OverlayEventKind::AskRequested.is_allowed_in(OverlayUiState::AttachOpen));
        assert!(OverlayEventKind::AskRequested.is_allowed_in(OverlayUiState::InstructionsOpen));
    }

    #[test]
    fn attach_files_allowed_from_idle_or_attach_open() {
        assert!(OverlayEventKind::AttachFilesRequested.is_allowed_in(OverlayUiState::Idle));
        assert!(OverlayEventKind::AttachFilesRequested.is_allowed_in(OverlayUiState::AttachOpen));
        assert!(
            !OverlayEventKind::AttachFilesRequested.is_allowed_in(OverlayUiState::InstructionsOpen)
        );
    }

    #[test]
    fn instructions_updated_only_allowed_when_instructions_open() {
        assert!(!OverlayEventKind::InstructionsUpdated.is_allowed_in(OverlayUiState::Idle));
        assert!(!OverlayEventKind::InstructionsUpdated.is_allowed_in(OverlayUiState::AttachOpen));
        assert!(
            OverlayEventKind::InstructionsUpdated.is_allowed_in(OverlayUiState::InstructionsOpen)
        );
    }

    #[test]
    fn overlong_question_rejected() {
        let cmd = OverlayIpcCommand::AskRequested {
            question: "x".repeat(MAX_QUESTION_LEN + 1),
        };
        assert!(validate_command_lengths(&cmd).is_err());
    }

    #[test]
    fn question_at_limit_accepted() {
        let cmd = OverlayIpcCommand::AskRequested {
            question: "x".repeat(MAX_QUESTION_LEN),
        };
        assert!(validate_command_lengths(&cmd).is_ok());
    }

    #[test]
    fn overlong_instructions_rejected() {
        let cmd = OverlayIpcCommand::InstructionsUpdated {
            instructions: "y".repeat(MAX_INSTRUCTIONS_LEN + 1),
        };
        assert!(validate_command_lengths(&cmd).is_err());
    }

    #[test]
    fn too_many_paths_rejected() {
        let cmd = OverlayIpcCommand::AttachFilesRequested {
            paths: vec!["a.txt".into(); MAX_PATHS_COUNT + 1],
        };
        assert!(validate_command_lengths(&cmd).is_err());
    }

    #[test]
    fn overlong_path_rejected() {
        let cmd = OverlayIpcCommand::AttachFilesRequested {
            paths: vec!["p".repeat(MAX_PATH_LEN + 1)],
        };
        assert!(validate_command_lengths(&cmd).is_err());
    }

    #[test]
    fn overlong_transcript_text_rejected_at_decode() {
        let big_text = "z".repeat(MAX_TEXT_LEN + 1);
        let json = format!(
            r#"{{"type":"transcript_partial","source":"mic","text":"{}"}}"#,
            big_text
        );
        let result = decode_ndjson(&json);
        assert!(result.is_err());
    }

    #[test]
    fn decode_command_ndjson_validates_lengths() {
        let big_q = "q".repeat(MAX_QUESTION_LEN + 1);
        let json = format!(r#"{{"type":"ask_requested","question":"{}"}}"#, big_q);
        let result = decode_command_ndjson(&json);
        assert!(result.is_err());
    }
}
