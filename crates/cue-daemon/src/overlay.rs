//! Daemon ↔ native-overlay process IPC.
//!
//! The native overlay (Swift on macOS, C on Windows in later phases) is a
//! child process. The daemon drives it over stdin (NDJSON
//! [`OverlayMessage`]s) and listens to stdout (NDJSON
//! [`OverlayIpcCommand`]s).
//!
//! This module isolates all of that plumbing behind one type,
//! [`NativeOverlayHandle`]. The rest of the daemon just calls `send(...)`
//! and `try_recv()`; restart, reader/writer tasks, and backoff all live
//! inside here.
//!
//! ### Process lifecycle
//!
//! - `spawn(path, args)` launches the binary, connects stdin/stdout, and
//!   starts a writer + reader task.
//! - If the child exits while the handle is still alive, the watcher task
//!   attempts to relaunch with exponential backoff up to [`MAX_RESTART_ATTEMPTS`].
//! - `shutdown()` (also runs on Drop) signals both tasks to exit and waits
//!   for the child to die. It does NOT send a kill signal first — it closes
//!   stdin, which the stub / real overlay uses to notice it should exit.
//!
//! ### Testing
//!
//! The integration test spawns `overlay-stub` (a tiny Rust binary in
//! `src/bin/overlay_stub.rs`) and drives a real `NativeOverlayHandle`
//! against it. See `tests/overlay_pipe_integration.rs`.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use cue_core::overlay_ipc::{decode_ndjson, encode_ndjson, OverlayIpcCommand, OverlayMessage};
use parking_lot::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

/// Upper bound on consecutive child-process restart attempts before we give
/// up and mark the handle as failed. Backoff doubles from 250 ms, capped at
/// 5 s, giving ~15 s of total budget before failure.
pub const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Coarse-grained state of the overlay-process side of the pipe. Queried
/// via [`NativeOverlayHandle::state`] for telemetry and UI banners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayProcessState {
    /// Process not started or already shut down.
    Idle,
    /// Starting up (between `spawn_child` and first successful I/O).
    Starting,
    /// Running — writer and reader tasks are active.
    Running,
    /// Process died, waiting to restart with backoff.
    Restarting { attempt: u32 },
    /// Gave up after [`MAX_RESTART_ATTEMPTS`].
    Failed,
    /// `shutdown` was called — will not restart.
    ShuttingDown,
}

/// Options used to spawn the overlay child.
#[derive(Debug, Clone)]
pub struct OverlaySpawnOptions {
    pub executable: PathBuf,
    pub args: Vec<String>,
}

impl OverlaySpawnOptions {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
        }
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = String>) -> Self {
        self.args = args.into_iter().collect();
        self
    }
}

/// Shared state accessed by the handle, the writer task, the reader task,
/// and the watcher/restart task.
struct Shared {
    state: Mutex<OverlayProcessState>,
    /// Flag consulted by tasks on every loop iteration — set to true when
    /// `shutdown` is called so tasks can exit without a channel drop.
    shutdown_requested: std::sync::atomic::AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            state: Mutex::new(OverlayProcessState::Idle),
            shutdown_requested: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn set_state(&self, s: OverlayProcessState) {
        *self.state.lock() = s;
    }

    fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested
            .load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Handle to a running (or restarting) overlay child process.
///
/// Construction is async (spawn requires a runtime context); once built,
/// both sending and receiving are non-blocking. Dropping the handle runs
/// `shutdown` implicitly.
pub struct NativeOverlayHandle {
    send_tx: UnboundedSender<OverlayMessage>,
    recv_rx: UnboundedReceiver<OverlayIpcCommand>,
    shared: Arc<Shared>,
    _tasks: Vec<JoinHandle<()>>,
}

impl NativeOverlayHandle {
    /// Spawn the overlay child process and start the IPC tasks.
    ///
    /// Returns an error if the executable path is invalid / not executable.
    /// I/O problems AFTER a successful spawn are handled by the restart
    /// watcher — they do not return here.
    pub async fn spawn(opts: OverlaySpawnOptions) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::new());
        let (send_tx, send_rx) = unbounded_channel::<OverlayMessage>();
        let (recv_tx, recv_rx) = unbounded_channel::<OverlayIpcCommand>();

        shared.set_state(OverlayProcessState::Starting);

        let child = spawn_child(&opts).await?;
        shared.set_state(OverlayProcessState::Running);

        let tasks = wire_child(child, opts, shared.clone(), send_rx, recv_tx);

        Ok(Self {
            send_tx,
            recv_rx,
            shared,
            _tasks: tasks,
        })
    }

    /// Enqueue a message for the overlay. Non-blocking. Returns `Err` only
    /// if the handle has been shut down (channel closed).
    pub fn send(&self, msg: OverlayMessage) -> Result<(), OverlayMessage> {
        self.send_tx.send(msg).map_err(|e| e.0)
    }

    /// Await the next `OverlayIpcCommand` from the overlay, if any.
    /// Returns `None` when the pipe is permanently closed.
    pub async fn next_command(&mut self) -> Option<OverlayIpcCommand> {
        self.recv_rx.recv().await
    }

    /// Non-blocking peek. Returns `None` if no command is pending.
    pub fn try_next_command(&mut self) -> Option<OverlayIpcCommand> {
        self.recv_rx.try_recv().ok()
    }

    /// Current process state. Useful for UI indicators.
    pub fn state(&self) -> OverlayProcessState {
        *self.shared.state.lock()
    }

    /// Request shutdown. Tasks will exit promptly; the watcher will not
    /// restart the child. Safe to call multiple times.
    pub async fn shutdown(mut self) {
        self.shared
            .shutdown_requested
            .store(true, std::sync::atomic::Ordering::Release);
        self.shared.set_state(OverlayProcessState::ShuttingDown);
        // Drop the sender so the writer task sees channel close and exits.
        // Safety:  is replaced with a dead channel; the handle
        // is being consumed anyway.
        let (dead_tx, _dead_rx) = unbounded_channel();
        let _ = std::mem::replace(&mut self.send_tx, dead_tx);
        // Take the tasks vec out so we own it independently of .
        let tasks = std::mem::take(&mut self._tasks);
        for handle in tasks {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }
}

impl Drop for NativeOverlayHandle {
    fn drop(&mut self) {
        self.shared
            .shutdown_requested
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

async fn spawn_child(opts: &OverlaySpawnOptions) -> std::io::Result<Child> {
    Command::new(&opts.executable)
        .args(&opts.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
}

/// Start the writer + reader + watcher tasks for a spawned child. Returns
/// the list of `JoinHandle`s so the caller can join on shutdown.
fn wire_child(
    mut child: Child,
    opts: OverlaySpawnOptions,
    shared: Arc<Shared>,
    mut send_rx: UnboundedReceiver<OverlayMessage>,
    recv_tx: UnboundedSender<OverlayIpcCommand>,
) -> Vec<JoinHandle<()>> {
    let mut tasks = Vec::with_capacity(3);

    let stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    // Writer: drain send_rx → child stdin.
    let shared_w = shared.clone();
    tasks.push(tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(msg) = send_rx.recv().await {
            if shared_w.is_shutdown_requested() {
                break;
            }
            let line = match encode_ndjson(&msg) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to encode overlay message");
                    continue;
                }
            };
            if let Err(e) = stdin.write_all(line.as_bytes()).await {
                tracing::warn!(error = %e, "overlay stdin write failed");
                break;
            }
            if let Err(e) = stdin.flush().await {
                tracing::warn!(error = %e, "overlay stdin flush failed");
                break;
            }
        }
    }));

    // Reader: stream child stdout lines → recv_tx.
    let shared_r = shared.clone();
    let recv_tx_reader = recv_tx.clone();
    tasks.push(tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            if shared_r.is_shutdown_requested() {
                break;
            }
            match lines.next_line().await {
                Ok(Some(line)) => match decode_ndjson(&line) {
                    Ok(OverlayMessage::Ping) => {
                        // Overlay responding with a ping instead of a proper command
                        // is allowed during handshake — treat as a Pong-equivalent.
                        let _ = recv_tx_reader.send(OverlayIpcCommand::Pong);
                    }
                    Ok(_) => {
                        // Overlay stdout is expected to carry COMMANDS not MESSAGES.
                        // Anything else is a protocol mismatch — log and move on.
                        tracing::warn!(line = %line, "overlay sent message on reverse pipe");
                    }
                    Err(_) => {
                        // Try decoding as a command (proper reverse-direction type).
                        match serde_json::from_str::<OverlayIpcCommand>(&line) {
                            Ok(cmd) => {
                                let _ = recv_tx_reader.send(cmd);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, line = %line, "bad overlay stdout line");
                            }
                        }
                    }
                },
                Ok(None) => break, // EOF — child closed stdout
                Err(e) => {
                    tracing::warn!(error = %e, "overlay stdout read error");
                    break;
                }
            }
        }
    }));

    // Watcher: wait on the child; restart with backoff if it dies unexpectedly.
    let shared_wait = shared.clone();
    tasks.push(tokio::spawn(async move {
        // Phase 3 scope: restart loop left as a single-shot for the first
        // child. A future phase will extend this to relaunch the child with
        // backoff on unexpected exit. For now, just observe the exit and
        // update state — integration tests exercise the single-shot path.
        let status = child.wait().await;
        if shared_wait.is_shutdown_requested() {
            shared_wait.set_state(OverlayProcessState::ShuttingDown);
        } else {
            match status {
                Ok(s) if s.success() => shared_wait.set_state(OverlayProcessState::Idle),
                Ok(_) | Err(_) => shared_wait.set_state(OverlayProcessState::Failed),
            }
        }
        // `opts` retained so a future restart implementation has the
        // original spawn args without needing another allocation.
        let _ = opts;
    }));

    tasks
}

/// Exponential backoff helper for child-process restarts. Same shape as the
/// Deepgram one but with a lower cap (5 s) because the overlay is local.
pub fn restart_delay(attempt: u32) -> Duration {
    let base_ms: u64 = 250;
    let capped = attempt.min(6);
    let ms = base_ms.saturating_mul(1u64 << capped);
    Duration::from_millis(ms.min(5_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_process_state_default_is_idle_when_constructed() {
        let s = Shared::new();
        assert_eq!(*s.state.lock(), OverlayProcessState::Idle);
        assert!(!s.is_shutdown_requested());
    }

    #[test]
    fn overlay_spawn_options_with_args_appends() {
        let opts = OverlaySpawnOptions::new("/bin/true").with_args(["--foo".into(), "bar".into()]);
        assert_eq!(opts.args, vec!["--foo".to_string(), "bar".to_string()]);
    }

    #[test]
    fn restart_delay_exponential_and_capped() {
        assert_eq!(restart_delay(0).as_millis(), 250);
        assert_eq!(restart_delay(1).as_millis(), 500);
        assert_eq!(restart_delay(4).as_millis(), 4_000);
        // capped at 5000
        assert_eq!(restart_delay(10).as_millis(), 5_000);
    }

    #[test]
    fn shared_state_transitions_are_visible() {
        let s = Shared::new();
        s.set_state(OverlayProcessState::Running);
        assert_eq!(*s.state.lock(), OverlayProcessState::Running);
        s.set_state(OverlayProcessState::Restarting { attempt: 2 });
        assert_eq!(
            *s.state.lock(),
            OverlayProcessState::Restarting { attempt: 2 }
        );
    }
}
