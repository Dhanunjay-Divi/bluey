//! Integration tests for overlay supervisor crash-loop cap exhaustion
//! and long-running child clean shutdown.

use std::time::Duration;

use cue_core::overlay_ipc::{OverlayIpcCommand, OverlayMessage};
use cue_daemon::overlay::{NativeOverlayHandle, OverlayProcessState, OverlaySpawnOptions};

fn crash_stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub-crash").to_string()
}

fn long_stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub-long").to_string()
}

/// After MAX_RESTART_ATTEMPTS consecutive failures the supervisor must
/// reach Failed state and stay there (no further restarts, no zombie tasks).
#[tokio::test]
async fn overlay_supervisor_caps_at_max_restart_attempts() {
    let _ = tracing_subscriber::fmt::try_init();
    // Use a stub that exits immediately with non-zero — every spawn is a
    // consecutive failure since the child never processes anything.
    let opts = OverlaySpawnOptions::new(crash_stub_path());
    let handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

    // Wait for state to reach Failed (supervisor exhausts restart attempts).
    let reached_failed = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if handle.state() == OverlayProcessState::Failed {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or(false);

    assert!(reached_failed, "supervisor did not reach Failed state");

    // Assert state stays Failed for 1 second (no further restarts).
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(handle.state(), OverlayProcessState::Failed);

    // send_tx should now fail (supervisor exited, recv_tx dropped).
    let send_result = handle.send(OverlayMessage::Ping);
    assert!(
        send_result.is_err(),
        "send should fail after cap exhaustion"
    );
}

/// A long-running child that exits cleanly on stdin EOF should be shut down
/// gracefully by shutdown(). The child exits with code 0.
#[tokio::test]
async fn overlay_long_running_child_shuts_down_cleanly() {
    let _ = tracing_subscriber::fmt::try_init();
    let opts = OverlaySpawnOptions::new(long_stub_path());
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

    assert_eq!(handle.state(), OverlayProcessState::Running);

    // Send 10 messages and collect 10 Pong + 10 Echo responses.
    for i in 0..10 {
        handle
            .send(OverlayMessage::SessionSwitched {
                session_id: Some(format!("sess-{i}")),
                title: Some(format!("title-{i}")),
            })
            .expect("send");
    }

    let mut pongs = 0u32;
    let mut echoes = 0u32;
    let all_received = tokio::time::timeout(Duration::from_secs(5), async {
        while pongs < 10 || echoes < 10 {
            match handle.next_command().await {
                Some(OverlayIpcCommand::Pong) => pongs += 1,
                Some(OverlayIpcCommand::Echo { .. }) => echoes += 1,
                Some(_) => {}
                None => break,
            }
        }
    })
    .await;
    assert!(all_received.is_ok(), "timed out waiting for responses");
    assert_eq!(pongs, 10);
    assert_eq!(echoes, 10);

    // Shutdown should complete within 3 seconds.
    let shutdown_result = tokio::time::timeout(Duration::from_secs(3), handle.shutdown()).await;
    assert!(
        shutdown_result.is_ok(),
        "shutdown did not complete within 3 seconds"
    );
}
