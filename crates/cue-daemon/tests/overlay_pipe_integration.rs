//! Integration test: SessionSwitched round-trip through the native-overlay
//! pipe using the `overlay-stub` binary as a stand-in for the real Swift/C
//! overlay.
//!
//! Exercises:
//!   1. Spawning the child process
//!   2. Sending `OverlayMessage::SessionSwitched` over stdin as NDJSON
//!   3. Receiving `OverlayIpcCommand::Pong` back over stdout
//!   4. Receiving `OverlayIpcCommand::Echo { payload }` carrying the exact
//!      JSON-serialized message — proves payload fidelity, not just
//!      decodability of the variant
//!   5. Clean shutdown
//!
//! The stub binary's path is injected via `CARGO_BIN_EXE_overlay-stub`
//! (Cargo automatically sets this env var for any integration test in a
//! crate that has a matching `[[bin]]`).

use std::time::Duration;

use cue_core::overlay_ipc::{OverlayIpcCommand, OverlayMessage};
use cue_daemon::overlay::{NativeOverlayHandle, OverlayProcessState, OverlaySpawnOptions};

fn stub_path() -> String {
    // Cargo sets this for every bin in the crate under test.
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

    // Send three SessionSwitched events with distinct payloads.
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

    // Each message yields two commands: Pong then Echo{payload}.
    let commands = collect_commands(&mut handle, messages.len() * 2, Duration::from_secs(3)).await;

    // Verify the interleaved (Pong, Echo) pattern AND that each Echo
    // payload round-trips to the exact OverlayMessage we sent.
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

    // Shutdown should return within the handle's 2-second-per-task budget.
    tokio::time::timeout(Duration::from_secs(5), handle.shutdown())
        .await
        .expect("shutdown did not complete in time");
}
