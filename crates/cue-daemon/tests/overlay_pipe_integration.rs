//! Integration test: SessionSwitched round-trip through the native-overlay
//! pipe using the  binary as a stand-in for the real Swift/C
//! overlay.
//!
//! Exercises:
//!   1. Spawning the child process
//!   2. Sending  over stdin as NDJSON
//!   3. Receiving  back over stdout
//!   4. Receiving  carrying the exact
//!      JSON-serialized message
//!   5. Clean shutdown
//!   6. Session token validation (Item 3 hardening)

use std::time::Duration;

use cue_core::overlay_ipc::{OverlayIpcCommand, OverlayMessage};
use cue_daemon::overlay::{
    generate_session_token, NativeOverlayHandle, OverlayProcessState, OverlaySpawnOptions,
};

fn stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub").to_string()
}

async fn collect_commands(
    handle: &mut NativeOverlayHandle,
    count: usize,
    per_cmd_timeout: Duration,
) -> Vec<OverlayIpcCommand> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let cmd = tokio::time::timeout(per_cmd_timeout, handle.next_command())
            .await
            .expect("stub response timeout")
            .expect("stub channel closed unexpectedly");
        out.push(cmd);
    }
    out
}

#[tokio::test]
async fn session_switched_round_trips_through_overlay_pipe() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn stub");

    assert_eq!(handle.state(), OverlayProcessState::Running);

    let messages = vec![
        OverlayMessage::SessionSwitched {
            session_id: Some("abc-001".into()),
            title: Some("first".into()),
        },
        OverlayMessage::SessionSwitched {
            session_id: None,
            title: None,
        },
        OverlayMessage::SessionSwitched {
            session_id: Some("xyz-777".into()),
            title: Some("third".into()),
        },
    ];

    for msg in &messages {
        handle.send(msg.clone()).expect("send to stub");
    }

    let commands = collect_commands(&mut handle, messages.len() * 2, Duration::from_secs(3)).await;

    for (i, expected) in messages.iter().enumerate() {
        let pong = &commands[i * 2];
        let echo = &commands[i * 2 + 1];
        assert_eq!(pong, &OverlayIpcCommand::Pong, "message {i}: missing Pong");
        let OverlayIpcCommand::Echo { payload } = echo else {
            panic!("message {i}: expected Echo, got {echo:?}");
        };
        let decoded: OverlayMessage = serde_json::from_str(payload)
            .unwrap_or_else(|e| panic!("message {i}: bad Echo payload {e}: {payload}"));
        assert_eq!(&decoded, expected, "message {i}: payload was not preserved");
    }

    handle.shutdown().await;
}

#[tokio::test]
async fn transcript_partial_round_trips_through_overlay_pipe() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.unwrap();

    let sent = OverlayMessage::TranscriptPartial {
        source: "microphone".into(),
        text: "hello wor".into(),
    };
    handle.send(sent.clone()).unwrap();

    let commands = collect_commands(&mut handle, 2, Duration::from_secs(3)).await;
    assert_eq!(commands[0], OverlayIpcCommand::Pong);
    let OverlayIpcCommand::Echo { payload } = &commands[1] else {
        panic!("expected Echo, got {:?}", commands[1]);
    };
    let decoded: OverlayMessage = serde_json::from_str(payload).unwrap();
    assert_eq!(decoded, sent);

    handle.shutdown().await;
}

#[tokio::test]
async fn overlay_shutdown_is_graceful() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let handle = NativeOverlayHandle::spawn(opts).await.unwrap();
    assert_eq!(handle.state(), OverlayProcessState::Running);

    tokio::time::timeout(Duration::from_secs(5), handle.shutdown())
        .await
        .expect("shutdown did not complete in time");
}

// Item 3: Token handshake integration tests

#[tokio::test]
async fn token_handshake_valid_token_accepted() {
    let token = generate_session_token();
    let opts = OverlaySpawnOptions::new(stub_path()).with_session_token(token);
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

    handle.send(OverlayMessage::Ping).expect("send");

    let cmd = tokio::time::timeout(Duration::from_secs(3), handle.next_command())
        .await
        .expect("timeout")
        .expect("channel closed");
    assert_eq!(cmd, OverlayIpcCommand::Pong);

    handle.shutdown().await;
}

#[tokio::test]
async fn token_handshake_no_token_legacy_accepted_when_disabled() {
    // When session_token is empty, legacy messages are accepted
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

    handle.send(OverlayMessage::Ping).expect("send");

    let cmd = tokio::time::timeout(Duration::from_secs(3), handle.next_command())
        .await
        .expect("timeout")
        .expect("channel closed");
    assert_eq!(cmd, OverlayIpcCommand::Pong);

    handle.shutdown().await;
}
