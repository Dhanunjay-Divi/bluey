//! Integration tests for overlay supervisor crash-loop cap exhaustion
//! and long-running child clean shutdown.

use std::time::Duration;

use cue_core::overlay_ipc::{OverlayIpcCommand, OverlayMessage};
use cue_daemon::overlay::{
    generate_session_token, NativeOverlayHandle, OverlayProcessState, OverlaySpawnOptions,
};

fn crash_stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub-crash").to_string()
}

fn long_stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub-long").to_string()
}

/// After MAX_RESTART_ATTEMPTS consecutive failures the supervisor must
/// reach Failed state and stay there.
#[tokio::test]
async fn overlay_supervisor_caps_at_max_restart_attempts() {
    let _ = tracing_subscriber::fmt::try_init();
    let opts = OverlaySpawnOptions::new(crash_stub_path());
    let handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

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

    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(handle.state(), OverlayProcessState::Failed);

    let send_result = handle.send(OverlayMessage::Ping);
    assert!(
        send_result.is_err(),
        "send should fail after cap exhaustion"
    );
}

/// Long-running child with token: exits cleanly on stdin EOF.
#[tokio::test]
async fn overlay_long_running_child_shuts_down_cleanly() {
    let _ = tracing_subscriber::fmt::try_init();
    let token = generate_session_token().expect("generate token");
    let opts = OverlaySpawnOptions::new(long_stub_path()).with_session_token(token);
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

    assert_eq!(handle.state(), OverlayProcessState::Running);

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

    let shutdown_result = tokio::time::timeout(Duration::from_secs(3), handle.shutdown()).await;
    assert!(
        shutdown_result.is_ok(),
        "shutdown did not complete within 3 seconds"
    );
}
