//! Restart-loop integration test for the overlay supervisor.
//!
//! Uses overlay-stub-oneshot, a stub variant that exits non-zero after
//! sending one Pong. The test sends two messages; if the supervisor restart
//! loop works, the second message reaches a freshly respawned child.

use std::time::Duration;

use cue_core::overlay_ipc::{OverlayIpcCommand, OverlayMessage};
use cue_daemon::overlay::{
    generate_session_token, NativeOverlayHandle, OverlayProcessState, OverlaySpawnOptions,
};

fn stub_path() -> String {
    env!("CARGO_BIN_EXE_overlay-stub-oneshot").to_string()
}

#[tokio::test]
async fn overlay_supervisor_respawns_on_unexpected_exit() {
    let _ = tracing_subscriber::fmt::try_init();
    let token = generate_session_token();
    let opts = OverlaySpawnOptions::new(stub_path()).with_session_token(token);
    let mut handle = NativeOverlayHandle::spawn(opts).await.expect("spawn");

    assert_eq!(handle.state(), OverlayProcessState::Running);

    handle
        .send(OverlayMessage::SessionSwitched {
            session_id: Some("first".into()),
            title: Some("before crash".into()),
        })
        .expect("send first");

    let pong1 = tokio::time::timeout(Duration::from_secs(3), handle.next_command())
        .await
        .expect("first pong timeout")
        .expect("channel closed");
    assert_eq!(pong1, OverlayIpcCommand::Pong);

    let restart_observed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match handle.state() {
                OverlayProcessState::Running => return true,
                OverlayProcessState::Failed => return false,
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .expect("supervisor never settled");
    assert!(
        restart_observed,
        "supervisor entered Failed instead of restarting"
    );

    handle
        .send(OverlayMessage::SessionSwitched {
            session_id: Some("second".into()),
            title: Some("after restart".into()),
        })
        .expect("send second");

    let pong2 = tokio::time::timeout(Duration::from_secs(5), handle.next_command())
        .await
        .expect("second pong timeout")
        .expect("channel closed");
    assert_eq!(pong2, OverlayIpcCommand::Pong);

    handle.shutdown().await;
}
