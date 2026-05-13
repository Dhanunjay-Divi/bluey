//! Integration test: SessionSwitched round-trip through the native-overlay
//! pipe using the `overlay-stub` binary as a stand-in for the real Swift/C
//! overlay.
//!
//! Exercises:
//!   1. Spawning the child process
//!   2. Sending `OverlayMessage::SessionSwitched` over stdin as NDJSON
//!   3. Receiving `OverlayIpcCommand::Pong` back over stdout
//!   4. Clean shutdown
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

#[tokio::test]
async fn session_switched_round_trips_through_overlay_pipe() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn stub");

    // Just after spawn, running.
    assert_eq!(handle.state(), OverlayProcessState::Running);

    // Send three SessionSwitched events — expect three Pongs.
    for (id, title) in [
        (Some("abc-001".to_string()), Some("first".to_string())),
        (None, None),
        (Some("xyz-777".to_string()), Some("third".to_string())),
    ] {
        handle
            .send(OverlayMessage::SessionSwitched {
                session_id: id,
                title,
            })
            .expect("send to stub");
    }

    for _ in 0..3 {
        let cmd = tokio::time::timeout(Duration::from_secs(3), handle.next_command())
            .await
            .expect("stub response timeout")
            .expect("stub channel closed unexpectedly");
        assert_eq!(cmd, OverlayIpcCommand::Pong);
    }

    handle.shutdown().await;
}

#[tokio::test]
async fn transcript_partial_round_trips_through_overlay_pipe() {
    let opts = OverlaySpawnOptions::new(stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.unwrap();

    handle
        .send(OverlayMessage::TranscriptPartial {
            source: "microphone".into(),
            text: "hello wor".into(),
        })
        .unwrap();

    let cmd = tokio::time::timeout(Duration::from_secs(3), handle.next_command())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cmd, OverlayIpcCommand::Pong);

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
