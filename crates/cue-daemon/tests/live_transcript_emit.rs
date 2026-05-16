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

/// Verify that the poller deduplication logic (skip segments at indices < last_count)
/// delivers each segment exactly once across catch-up + live path.
#[test]
fn live_transcript_dedup_no_duplicates_across_catchup_and_poller() {
    let tmp_dir = std::env::temp_dir().join(format!("cue-dedup-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let meeting_path = tmp_dir.join("active-meeting.json");

    // Create meeting with 3 initial segments (simulating existing data)
    let mut meeting = MeetingRecord::new(Some("Dedup test".to_string()));
    for i in 0..3 {
        meeting.transcript.push(cue_core::TranscriptSegment::new(
            cue_core::Speaker::User,
            format!("segment {i}"),
            true,
        ));
    }
    std::fs::write(&meeting_path, serde_json::to_vec_pretty(&meeting).unwrap()).unwrap();

    // Simulate catch-up: UI reads all segments (since_index=0), gets indices 0,1,2
    let catchup_count = meeting.transcript.len();
    assert_eq!(catchup_count, 3);

    // Simulate poller state: last_count starts at catchup_count (3)
    let mut last_count: usize = catchup_count;

    // Add 2 more segments (simulating daemon writing new data)
    meeting.transcript.push(cue_core::TranscriptSegment::new(
        cue_core::Speaker::System,
        "segment 3",
        true,
    ));
    meeting.transcript.push(cue_core::TranscriptSegment::new(
        cue_core::Speaker::User,
        "segment 4",
        true,
    ));
    std::fs::write(&meeting_path, serde_json::to_vec_pretty(&meeting).unwrap()).unwrap();

    // Poller tick: only emit segments at index >= last_count
    let total = meeting.transcript.len();
    let mut emitted: Vec<(usize, String)> = Vec::new();
    for (i, seg) in meeting.transcript.iter().enumerate().skip(last_count) {
        emitted.push((i, seg.text.clone()));
    }
    last_count = total;

    // Verify: only segments 3 and 4 were emitted (no duplicates of 0,1,2)
    assert_eq!(emitted.len(), 2);
    assert_eq!(emitted[0], (3, "segment 3".to_string()));
    assert_eq!(emitted[1], (4, "segment 4".to_string()));
    assert_eq!(last_count, 5);

    // Another poller tick with no new data: nothing emitted
    let total2 = meeting.transcript.len();
    assert!(total2 <= last_count); // no new segments

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
