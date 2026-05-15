//! Integration test: verify that the live transcript event infrastructure
//! works correctly — serialization, broadcast delivery, and meeting persistence.

use cue_core::MeetingRecord;

/// Verify the LiveTranscriptEvent struct serializes to the expected JSON shape
/// matching the contract consumed by the dashboard UI.
#[test]
fn live_transcript_event_serializes_correctly() {
    let event = cue_daemon::app::LiveTranscriptEvent {
        session_id: "abc-123".to_string(),
        source: "microphone".to_string(),
        text: "hello world".to_string(),
        is_final: true,
        speaker: None,
        ts_ms: 1715792400000,
    };
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["session_id"], "abc-123");
    assert_eq!(json["source"], "microphone");
    assert_eq!(json["text"], "hello world");
    assert_eq!(json["is_final"], true);
    assert!(json["speaker"].is_null());
    assert_eq!(json["ts_ms"], 1715792400000u64);
}

/// Verify the broadcast channel delivers events when subscribed — this is the
/// same channel type used in the Daemon struct to relay transcript events.
#[tokio::test]
async fn broadcast_channel_delivers_live_transcript_event() {
    use tokio::sync::broadcast;

    let (tx, mut rx) = broadcast::channel::<cue_daemon::app::LiveTranscriptEvent>(16);

    let event = cue_daemon::app::LiveTranscriptEvent {
        session_id: "sess-1".to_string(),
        source: "system".to_string(),
        text: "test segment".to_string(),
        is_final: false,
        speaker: None,
        ts_ms: 1000,
    };

    tx.send(event.clone()).unwrap();
    let received = rx.recv().await.unwrap();
    assert_eq!(received.session_id, "sess-1");
    assert_eq!(received.source, "system");
    assert_eq!(received.text, "test segment");
    assert!(!received.is_final);
    assert_eq!(received.ts_ms, 1000);
}

/// Verify that transcript segments added to a MeetingRecord are persisted
/// correctly — this exercises the same serialization path used by
/// `add_audio_transcript_segment` in production.
#[test]
fn transcript_segment_persists_to_meeting_json() {
    let tmp_dir = std::env::temp_dir().join(format!("cue-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let meeting_path = tmp_dir.join("active-meeting.json");

    // Create and save a meeting with no transcripts.
    let meeting = MeetingRecord::new(Some("Test meeting".to_string()));
    let bytes = serde_json::to_vec_pretty(&meeting).unwrap();
    std::fs::write(&meeting_path, &bytes).unwrap();

    // Add a transcript segment (same as production code does).
    let mut meeting: MeetingRecord =
        serde_json::from_slice(&std::fs::read(&meeting_path).unwrap()).unwrap();
    assert_eq!(meeting.transcript.len(), 0);

    let segment = cue_core::TranscriptSegment::new(cue_core::Speaker::User, "hello world", true);
    meeting.transcript.push(segment);
    let bytes = serde_json::to_vec_pretty(&meeting).unwrap();
    std::fs::write(&meeting_path, &bytes).unwrap();

    // Reload and verify.
    let reloaded: MeetingRecord =
        serde_json::from_slice(&std::fs::read(&meeting_path).unwrap()).unwrap();
    assert_eq!(reloaded.transcript.len(), 1);
    assert_eq!(reloaded.transcript[0].text, "hello world");
    assert!(reloaded.transcript[0].is_final);

    // Cleanup.
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
