//! Integration tests for partial->final transcript deduplication.
//!
//! Verifies that when a final transcript arrives that supersedes a recent
//! partial from the same speaker, the partial is removed from the meeting
//! transcript to avoid duplicate display in the dashboard.

use cue_core::meeting::{MeetingRecord, Speaker, TranscriptSegment};
use cue_daemon::app::dedup_partial_on_final;

#[test]
fn final_supersedes_matching_partial() {
    let mut meeting = MeetingRecord::new(Some("test".into()));
    meeting
        .transcript
        .push(TranscriptSegment::new(Speaker::User, "hello wor", false));

    let removed = dedup_partial_on_final(&mut meeting, Speaker::User, "hello world");
    assert!(
        removed,
        "partial should be removed when final supersedes it"
    );
    assert!(
        meeting.transcript.is_empty(),
        "partial should have been removed"
    );
}

#[test]
fn final_does_not_remove_unrelated_partial() {
    let mut meeting = MeetingRecord::new(Some("test".into()));
    meeting
        .transcript
        .push(TranscriptSegment::new(Speaker::User, "goodbye", false));

    let removed = dedup_partial_on_final(&mut meeting, Speaker::User, "hello world");
    assert!(!removed, "unrelated partial should not be removed");
    assert_eq!(meeting.transcript.len(), 1);
}

#[test]
fn final_does_not_remove_partial_from_different_speaker() {
    let mut meeting = MeetingRecord::new(Some("test".into()));
    meeting
        .transcript
        .push(TranscriptSegment::new(Speaker::System, "hello wor", false));

    let removed = dedup_partial_on_final(&mut meeting, Speaker::User, "hello world");
    assert!(
        !removed,
        "partial from different speaker should not be removed"
    );
    assert_eq!(meeting.transcript.len(), 1);
}

#[test]
fn final_does_not_remove_already_final_segment() {
    let mut meeting = MeetingRecord::new(Some("test".into()));
    meeting
        .transcript
        .push(TranscriptSegment::new(Speaker::User, "hello world", true));

    let removed = dedup_partial_on_final(&mut meeting, Speaker::User, "hello world again");
    assert!(!removed, "already-final segment should not be removed");
    assert_eq!(meeting.transcript.len(), 1);
}
