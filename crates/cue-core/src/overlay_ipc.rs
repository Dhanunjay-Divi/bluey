//! Wire protocol for daemon ↔ native-overlay IPC.
//!
//! The native Swift (macOS) and C (Windows) overlays live in separate
//! processes from `cue-daemon` and communicate via JSON over stdin/stdout.
//! Both sides must agree on the message schema, so it lives here in `cue-core`
//! (shared by the Rust sender and, once the Swift/C clients are updated, by
//! the receiver via equivalent definitions).
//!
//! Phase 3 adds `SessionSwitched` so the overlay can show the current session
//! id. Additional variants land as the listening pipeline starts emitting
//! transcript deltas, VAD state, STT connection banners, etc.

use serde::{Deserialize, Serialize};

/// A message pushed from `cue-daemon` to a native overlay process.
///
/// `#[serde(tag = "type")]` keeps the JSON form stable and self-describing:
///
/// ```text
/// {"type":"session_switched","session_id":"abc-...","title":"My session"}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayMessage {
    /// Active session changed. Overlay should update its session indicator.
    /// `session_id = None` means "no active session" (user just deleted the
    /// last one, etc.) — overlay should show a blank / "no session" badge.
    SessionSwitched {
        session_id: Option<String>,
        title: Option<String>,
    },

    /// Global listening state changed. Phase 3 does not emit these yet;
    /// defined now so the overlay schema versions cleanly when the pipeline
    /// starts reporting state.
    ListeningStateChanged { state: ListeningState },

    /// Live STT partial transcript delta. Phase 3+ emits these; overlay
    /// renders the rolling transcript. `source` identifies mic vs system.
    TranscriptPartial { source: String, text: String },

    /// Finalized transcript for a completed utterance.
    TranscriptFinal { source: String, text: String },

    /// Health check ping, for the overlay to acknowledge so daemon knows
    /// the receiver is alive. No payload needed.
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

/// Messages the overlay can send BACK to the daemon (IPC). Kept minimal today —
/// overlays are mostly display-only. Inputs arrive through the dashboard or
/// the global hotkey channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OverlayIpcCommand {
    /// Overlay acknowledges a `Ping` so daemon can tell it is alive.
    Pong,
    /// Overlay requests the daemon resend its current state (useful when
    /// the overlay launches after the daemon was already running).
    RequestSync,
    /// Debug echo. Payload is the JSON form of a recently-received
    /// `OverlayMessage` as observed by the overlay. Overlays can opt in
    /// for observability / integration tests; the daemon logs these at
    /// info level but otherwise ignores them.
    Echo { payload: String },
}

/// Serialize an `OverlayMessage` to a single-line NDJSON string (newline
/// appended). This is the exact format the daemon writes to the overlay's
/// stdin and lets the receiver read messages line-by-line.
pub fn encode_ndjson(msg: &OverlayMessage) -> Result<String, serde_json::Error> {
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    Ok(s)
}

/// Parse a single NDJSON line (with or without trailing newline) into an
/// `OverlayMessage`.
pub fn decode_ndjson(line: &str) -> Result<OverlayMessage, serde_json::Error> {
    serde_json::from_str(line.trim_end_matches('\n'))
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
}
