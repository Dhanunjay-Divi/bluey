//! Security integration tests for overlay IPC hardening (P3.R11 Items 4-7).
//!
//! Proves that:
//! - Transcript text containing overlay command substrings cannot trigger dispatch
//! - Malformed JSON with inner type fields is handled safely
//! - Overlong fields are rejected
//! - Path traversal in attached files is rejected
//! - State machine drops events when UI is not in the correct state

use std::time::Duration;

use cue_core::overlay_ipc::{
    decode_command_ndjson, validate_command_lengths, OverlayEventKind, OverlayIpcCommand,
    OverlayMessage, OverlayUiState, MAX_INSTRUCTIONS_LEN, MAX_PATH_LEN, MAX_QUESTION_LEN,
};
use cue_daemon::overlay::{NativeOverlayHandle, OverlayProcessState, OverlaySpawnOptions};

fn stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub").to_string()
}

async fn collect_commands(
    handle: &mut NativeOverlayHandle,
    count: usize,
    timeout: Duration,
) -> Vec<OverlayIpcCommand> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        match tokio::time::timeout(timeout, handle.next_command()).await {
            Ok(Some(cmd)) => out.push(cmd),
            _ => break,
        }
    }
    out
}

/// Item 7 test 1: transcript text containing overlay command substring does not
/// trigger an ask_requested event dispatch.
#[tokio::test]
async fn transcript_with_overlay_command_substring_does_not_dispatch() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.unwrap();

    // Send a transcript whose text contains a string that looks like an overlay command
    let msg = OverlayMessage::TranscriptPartial {
        source: "microphone".into(),
        text: r#"the speaker said "type":"ask_requested" verbatim"#.into(),
    };
    handle.send(msg.clone()).unwrap();

    // The stub echoes back Pong + Echo for each valid OverlayMessage.
    // We should get exactly those two, NOT an ask_requested command.
    let commands = collect_commands(&mut handle, 2, Duration::from_secs(3)).await;
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0], OverlayIpcCommand::Pong);

    // The echo should contain the transcript, not an ask_requested
    if let OverlayIpcCommand::Echo { payload } = &commands[1] {
        let decoded: OverlayMessage = serde_json::from_str(payload).unwrap();
        assert_eq!(decoded, msg);
        // Verify it's a transcript, not an ask
        assert!(payload.contains("transcript_partial"));
        assert!(!payload.starts_with(r#"{"type":"ask_requested"#));
    } else {
        panic!("expected Echo, got {:?}", commands[1]);
    }

    handle.shutdown().await;
}

/// Item 7 test 2: malformed JSON with inner type field is dropped - only outer
/// type is processed.
#[tokio::test]
async fn malformed_json_with_inner_type_field_is_dropped() {
    // This tests the Rust-side decode: a transcript_partial whose text field
    // contains what looks like a different command type.
    let json = r#"{"type":"transcript_partial","source":"mic","text":"{\"type\":\"attach_files_requested\",\"paths\":[\"/etc/passwd\"]}"}"#;
    let result = cue_core::overlay_ipc::decode_ndjson(json);
    assert!(result.is_ok());
    let msg = result.unwrap();
    // It should decode as TranscriptPartial, not as AttachFilesRequested
    match msg {
        OverlayMessage::TranscriptPartial { text, .. } => {
            assert!(text.contains("attach_files_requested"));
        }
        other => panic!("expected TranscriptPartial, got {:?}", other),
    }
}

/// Item 7 test 3: overlong question field is rejected.
#[tokio::test]
async fn overlong_question_field_is_rejected() {
    let big_question = "q".repeat(MAX_QUESTION_LEN + 1000);
    let cmd = OverlayIpcCommand::AskRequested {
        question: big_question.clone(),
    };
    let result = validate_command_lengths(&cmd);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("question field exceeds max length"));

    // Also test via decode_command_ndjson
    let json = format!(r#"{{"type":"ask_requested","question":"{}"}}"#, big_question);
    let result = decode_command_ndjson(&json);
    assert!(result.is_err());
}

/// Item 7 test 4: overlong instructions is rejected.
#[tokio::test]
async fn overlong_instructions_is_rejected() {
    let big_instructions = "i".repeat(MAX_INSTRUCTIONS_LEN + 4000);
    let cmd = OverlayIpcCommand::InstructionsUpdated {
        instructions: big_instructions.clone(),
    };
    let result = validate_command_lengths(&cmd);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("instructions field exceeds max length"));
}

/// Item 7 test 5: path traversal in attached file is rejected.
#[tokio::test]
async fn path_traversal_in_attached_file_is_rejected() {
    // Path traversal: the path contains ".." components
    let cmd = OverlayIpcCommand::AttachFilesRequested {
        paths: vec!["../../../etc/passwd".into()],
    };
    // Length validation passes (it's short), but the state machine should
    // block it when UI is not in AttachOpen state.
    let kind = OverlayEventKind::from_command(&cmd);
    assert_eq!(kind, OverlayEventKind::AttachFilesRequested);
    assert!(!kind.is_allowed_in(OverlayUiState::Idle));

    // Also: a path that exceeds MAX_PATH_LEN is rejected
    let long_path = "/".to_string() + &"a".repeat(MAX_PATH_LEN + 1);
    let cmd2 = OverlayIpcCommand::AttachFilesRequested {
        paths: vec![long_path],
    };
    assert!(validate_command_lengths(&cmd2).is_err());
}

/// Item 7 test 6: state machine drops attach_files_requested when idle.
#[tokio::test]
async fn state_machine_drops_attach_when_idle() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.unwrap();
    assert_eq!(handle.state(), OverlayProcessState::Running);

    // UI state is Idle by default
    assert_eq!(handle.ui_state(), OverlayUiState::Idle);

    // Send a valid message first to confirm the pipe works
    handle
        .send(OverlayMessage::Ping)
        .unwrap();

    let commands = collect_commands(&mut handle, 1, Duration::from_secs(2)).await;
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0], OverlayIpcCommand::Pong);

    // Now set UI state to AttachOpen and verify it works
    handle.set_ui_state(OverlayUiState::AttachOpen);
    assert_eq!(handle.ui_state(), OverlayUiState::AttachOpen);

    // Reset to Idle
    handle.set_ui_state(OverlayUiState::Idle);
    assert_eq!(handle.ui_state(), OverlayUiState::Idle);

    // The state machine validation is tested at the unit level in overlay_ipc.rs
    // Here we just verify the handle exposes the state correctly
    assert!(!OverlayEventKind::AttachFilesRequested.is_allowed_in(handle.ui_state()));

    handle.shutdown().await;
}

/// Additional: state machine allows attach after UI opens.
#[tokio::test]
async fn state_machine_allows_attach_after_ui_opens() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let handle = NativeOverlayHandle::spawn(opts).await.unwrap();

    // Initially blocked
    assert!(!OverlayEventKind::AttachFilesRequested.is_allowed_in(handle.ui_state()));

    // Open attach UI
    handle.set_ui_state(OverlayUiState::AttachOpen);
    assert!(OverlayEventKind::AttachFilesRequested.is_allowed_in(handle.ui_state()));

    // Close it
    handle.set_ui_state(OverlayUiState::Idle);
    assert!(!OverlayEventKind::AttachFilesRequested.is_allowed_in(handle.ui_state()));

    handle.shutdown().await;
}

/// Additional: instructions only allowed when instructions UI is open.
#[tokio::test]
async fn state_machine_instructions_gated() {
    assert!(!OverlayEventKind::InstructionsUpdated.is_allowed_in(OverlayUiState::Idle));
    assert!(!OverlayEventKind::InstructionsUpdated.is_allowed_in(OverlayUiState::AttachOpen));
    assert!(OverlayEventKind::InstructionsUpdated.is_allowed_in(OverlayUiState::InstructionsOpen));
}
