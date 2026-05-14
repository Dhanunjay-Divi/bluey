//! Daemon ↔ native-overlay process IPC.

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

pub const MAX_RESTART_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayProcessState {
    Idle,
    Starting,
    Running,
    Restarting { attempt: u32 },
    Failed,
    ShuttingDown,
}

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

struct Shared {
    state: Mutex<OverlayProcessState>,
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

pub struct NativeOverlayHandle {
    send_tx: UnboundedSender<OverlayMessage>,
    recv_rx: UnboundedReceiver<OverlayIpcCommand>,
    shared: Arc<Shared>,
    _tasks: Vec<JoinHandle<()>>,
}

impl NativeOverlayHandle {
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

    pub fn send(&self, msg: OverlayMessage) -> Result<(), OverlayMessage> {
        self.send_tx.send(msg).map_err(|e| e.0)
    }

    pub async fn next_command(&mut self) -> Option<OverlayIpcCommand> {
        self.recv_rx.recv().await
    }

    pub fn try_next_command(&mut self) -> Option<OverlayIpcCommand> {
        self.recv_rx.try_recv().ok()
    }

    pub fn state(&self) -> OverlayProcessState {
        *self.shared.state.lock()
    }

    pub async fn shutdown(mut self) {
        self.shared
            .shutdown_requested
            .store(true, std::sync::atomic::Ordering::Release);
        self.shared.set_state(OverlayProcessState::ShuttingDown);
        let (dead_tx, _dead_rx) = unbounded_channel();
        let _ = std::mem::replace(&mut self.send_tx, dead_tx);
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

fn wire_child(
    initial_child: Child,
    opts: OverlaySpawnOptions,
    shared: Arc<Shared>,
    send_rx: UnboundedReceiver<OverlayMessage>,
    recv_tx: UnboundedSender<OverlayIpcCommand>,
) -> Vec<JoinHandle<()>> {
    vec![tokio::spawn(run_supervisor(
        initial_child,
        opts,
        shared,
        send_rx,
        recv_tx,
    ))]
}

/// One iteration of the supervisor: drives a child to completion.
///
/// The supervisor owns `send_rx` and writes directly to the child's stdin.
/// A `shutdown_notify` is used to signal the supervisor when the reader
/// detects EOF (child exited). The supervisor selects on `child.wait()`,
/// `send_rx.recv()`, and the EOF notify. When the child exits, any message
/// that was written to stdin but not confirmed read by the child is carried
/// over to the next generation.
async fn run_one_child(
    mut child: Child,
    shared: Arc<Shared>,
    mut send_rx: UnboundedReceiver<OverlayMessage>,
    recv_tx: UnboundedSender<OverlayIpcCommand>,
    carryover_in: Option<OverlayMessage>,
) -> (
    UnboundedReceiver<OverlayMessage>,
    Option<OverlayMessage>,
    bool,
) {
    tracing::debug!("run_one_child: starting new generation");
    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    // Track messages written since last child output (pong/response).
    // When child exits, the last unacknowledged message is carryover.
    let msgs_written = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let msgs_acked = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Reader task.
    let recv_tx_reader = recv_tx.clone();
    let shared_r = shared.clone();
    let msgs_acked_r = msgs_acked.clone();
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            if shared_r.is_shutdown_requested() {
                break;
            }
            match lines.next_line().await {
                Ok(Some(line)) => {
                    tracing::debug!(line = %line, "reader: got line");
                    // Child produced output — it has read all messages up to now.
                    msgs_acked_r.fetch_add(1, std::sync::atomic::Ordering::Release);
                    match decode_ndjson(&line) {
                        Ok(OverlayMessage::Ping) => {
                            let _ = recv_tx_reader.send(OverlayIpcCommand::Pong);
                        }
                        Ok(_) => {
                            tracing::warn!(line = %line, "overlay sent message on reverse pipe");
                        }
                        Err(_) => match serde_json::from_str::<OverlayIpcCommand>(&line) {
                            Ok(cmd) => {
                                let _ = recv_tx_reader.send(cmd);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, line = %line, "bad overlay stdout line");
                            }
                        },
                    }
                }
                Ok(None) => {
                    tracing::debug!("reader: EOF");
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "overlay stdout read error");
                    break;
                }
            }
        }
    });

    // Helper to write one message to stdin.
    async fn write_msg(stdin: &mut tokio::process::ChildStdin, msg: &OverlayMessage) -> bool {
        let line = match encode_ndjson(msg) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "failed to encode overlay message");
                return true; // encoding error — drop msg
            }
        };
        stdin.write_all(line.as_bytes()).await.is_ok() && stdin.flush().await.is_ok()
    }

    // Write carryover first.
    if let Some(ref msg) = carryover_in {
        tracing::debug!("supervisor: writing carryover msg");
        if !write_msg(&mut stdin, msg).await {
            drop(stdin);
            let _ = reader.await;
            let status = child.wait().await;
            let clean = matches!(&status, Ok(s) if s.success());
            return (send_rx, carryover_in, clean);
        }
        msgs_written.fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    // Main select loop.
    let mut wait_fut = Box::pin(child.wait());
    let mut last_msg: Option<OverlayMessage> = carryover_in;
    let mut write_failed = false;
    let mut rx_closed = false;

    let status = loop {
        if shared.is_shutdown_requested() || rx_closed {
            break (&mut wait_fut).await;
        }
        tokio::select! {
            biased;
            s = &mut wait_fut => break s,
            msg = send_rx.recv() => {
                match msg {
                    Some(m) => {
                        if !write_msg(&mut stdin, &m).await {
                            last_msg = Some(m);
                            write_failed = true;
                            break (&mut wait_fut).await;
                        }
                        msgs_written.fetch_add(1, std::sync::atomic::Ordering::Release);
                        last_msg = Some(m);
                    }
                    None => { rx_closed = true; }
                }
            }
        }
    };

    let clean_exit = matches!(&status, Ok(s) if s.success());

    // Drop stdin so reader sees EOF.
    drop(stdin);
    let _ = reader.await;

    // Determine carryover: if we wrote more messages than the child acked,
    // the last written message was likely not read.
    let written = msgs_written.load(std::sync::atomic::Ordering::Acquire);
    let acked = msgs_acked.load(std::sync::atomic::Ordering::Acquire);
    let carryover_out = if !clean_exit && (written > acked || write_failed) {
        last_msg
    } else {
        None
    };

    tracing::debug!(
        carryover = carryover_out.is_some(),
        clean = clean_exit,
        written = written,
        acked = acked,
        "run_one_child: finished"
    );
    (send_rx, carryover_out, clean_exit)
}

async fn run_supervisor(
    initial_child: Child,
    opts: OverlaySpawnOptions,
    shared: Arc<Shared>,
    mut send_rx: UnboundedReceiver<OverlayMessage>,
    recv_tx: UnboundedSender<OverlayIpcCommand>,
) {
    let mut current_child = initial_child;
    let mut consecutive_failures: u32 = 0;
    let mut carryover: Option<OverlayMessage> = None;

    loop {
        let (rx_back, carryover_out, clean_exit) = run_one_child(
            current_child,
            shared.clone(),
            send_rx,
            recv_tx.clone(),
            carryover.take(),
        )
        .await;
        send_rx = rx_back;
        carryover = carryover_out;

        if shared.is_shutdown_requested() {
            shared.set_state(OverlayProcessState::ShuttingDown);
            return;
        }

        if clean_exit {
            shared.set_state(OverlayProcessState::Idle);
            return;
        }

        consecutive_failures += 1;
        if consecutive_failures > MAX_RESTART_ATTEMPTS {
            tracing::error!(
                attempts = consecutive_failures,
                "overlay child failed too many times; giving up"
            );
            shared.set_state(OverlayProcessState::Failed);
            return;
        }

        let delay = restart_delay(consecutive_failures - 1);
        shared.set_state(OverlayProcessState::Restarting {
            attempt: consecutive_failures,
        });
        tracing::warn!(
            attempt = consecutive_failures,
            delay_ms = delay.as_millis() as u64,
            executable = %opts.executable.display(),
            "overlay child exited unexpectedly; respawning"
        );
        tokio::time::sleep(delay).await;

        match spawn_child(&opts).await {
            Ok(c) => {
                current_child = c;
                shared.set_state(OverlayProcessState::Running);
                consecutive_failures = 0;
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    attempt = consecutive_failures,
                    "overlay respawn failed; will retry"
                );
                if consecutive_failures > MAX_RESTART_ATTEMPTS {
                    shared.set_state(OverlayProcessState::Failed);
                    return;
                }
                let next_delay = restart_delay(consecutive_failures);
                tokio::time::sleep(next_delay).await;
                match spawn_child(&opts).await {
                    Ok(c) => {
                        current_child = c;
                        shared.set_state(OverlayProcessState::Running);
                        consecutive_failures = 0;
                    }
                    Err(e2) => {
                        tracing::error!(error = %e2, "overlay respawn failed twice; giving up");
                        shared.set_state(OverlayProcessState::Failed);
                        return;
                    }
                }
            }
        }
    }
}

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
