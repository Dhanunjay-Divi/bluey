//! Daemon ↔ native-overlay process IPC.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use cue_core::overlay_ipc::{decode_ndjson, encode_ndjson, OverlayEvent, OverlayIpcCommand, OverlayMessage};
use parking_lot::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

pub const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Length of the hex session token (32 bytes = 64 hex chars).
pub const SESSION_TOKEN_HEX_LEN: usize = 64;

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
    /// Session token passed to the overlay via env var. If empty, token
    /// validation is disabled (for tests that do not set it).
    pub session_token: String,
}

impl OverlaySpawnOptions {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            session_token: String::new(),
        }
    }

    pub fn with_args(mut self, args: impl IntoIterator<Item = String>) -> Self {
        self.args = args.into_iter().collect();
        self
    }

    pub fn with_session_token(mut self, token: String) -> Self {
        self.session_token = token;
        self
    }
}

// ─── Item 1: Production overlay-bin override gate ───────────────────────────

/// Resolve the overlay binary path. In production (release) builds, env var
/// overrides (BLUEY_OVERLAY_BIN / CUE_OVERLAY_BIN) are IGNORED unless
/// BLUEY_DEV_OVERLAY=1 is also set. In debug builds the override is always
/// accepted.
pub fn resolve_overlay_path(default: &Path) -> PathBuf {
    let dev_mode = is_dev_mode();
    if dev_mode {
        if let Some(p) = env_overlay_bin() {
            tracing::info!(path = %p.display(), "using overlay binary override (dev mode)");
            return p;
        }
    } else if let Some(p) = env_overlay_bin() {
        tracing::warn!(
            path = %p.display(),
            "BLUEY_OVERLAY_BIN/CUE_OVERLAY_BIN set but IGNORED in production build \
             (set BLUEY_DEV_OVERLAY=1 to enable)"
        );
    }
    default.to_path_buf()
}

/// Returns true when we should allow env-var overrides.
fn is_dev_mode() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    std::env::var("BLUEY_DEV_OVERLAY")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn env_overlay_bin() -> Option<PathBuf> {
    std::env::var_os("BLUEY_OVERLAY_BIN")
        .or_else(|| std::env::var_os("CUE_OVERLAY_BIN"))
        .map(PathBuf::from)
}

// ─── Item 2: Overlay binary verification ────────────────────────────────────

/// Error returned when overlay binary verification fails.
#[derive(Debug, thiserror::Error)]
pub enum OverlayVerifyError {
    #[error("overlay path is not absolute: {0}")]
    NotAbsolute(PathBuf),
    #[error("overlay path could not be canonicalized: {0}")]
    CanonicalizeFailed(std::io::Error),
    #[error("overlay binary is outside install directory: binary={binary}, install_dir={install_dir}")]
    OutsideInstallDir {
        binary: PathBuf,
        install_dir: PathBuf,
    },
    // Future: HashMismatch { expected: String, actual: String }
}

/// Verify that the overlay binary path is canonical and resides inside
/// `install_dir`. This prevents symlink/traversal attacks that could trick
/// the daemon into spawning an attacker-controlled binary.
///
/// Future enhancement: SHA-256 hash verification (placeholder structured
/// so enabling it requires only a single flag + embedded hash constant).
pub fn verify_overlay_binary(path: &Path, install_dir: &Path) -> Result<(), OverlayVerifyError> {
    if !path.is_absolute() {
        return Err(OverlayVerifyError::NotAbsolute(path.to_path_buf()));
    }
    let canonical = path
        .canonicalize()
        .map_err(OverlayVerifyError::CanonicalizeFailed)?;
    let canonical_install = install_dir
        .canonicalize()
        .map_err(OverlayVerifyError::CanonicalizeFailed)?;
    if !canonical.starts_with(&canonical_install) {
        return Err(OverlayVerifyError::OutsideInstallDir {
            binary: canonical,
            install_dir: canonical_install,
        });
    }
    Ok(())
}

// ─── Item 3: Session token generation ───────────────────────────────────────

/// Generate a cryptographically random 32-byte hex session token (64 hex chars).
pub fn generate_session_token() -> String {
    let id = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();
    // Two UUIDs = 32 bytes = 64 hex chars
    format!("{}{}", id.as_simple(), id2.as_simple())
}

// ─── Core overlay handle ────────────────────────────────────────────────────

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
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
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
    let mut cmd = Command::new(&opts.executable);
    cmd.args(&opts.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    // Item 3: pass session token via env var
    if !opts.session_token.is_empty() {
        cmd.env("BLUEY_OVERLAY_SESSION_TOKEN", &opts.session_token);
    }
    cmd.spawn()
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

/// Validate an overlay event token. Returns true if valid.
fn validate_token(event: &OverlayEvent, expected: &str) -> bool {
    if expected.is_empty() {
        // Token validation disabled (e.g. legacy tests without token).
        return true;
    }
    event.token == expected
}

/// One iteration of the supervisor: drives a child to completion.
async fn run_one_child(
    mut child: Child,
    shared: Arc<Shared>,
    mut send_rx: UnboundedReceiver<OverlayMessage>,
    recv_tx: UnboundedSender<OverlayIpcCommand>,
    carryover_in: Option<OverlayMessage>,
    session_token: &str,
) -> (
    UnboundedReceiver<OverlayMessage>,
    Option<OverlayMessage>,
    bool,
) {
    tracing::debug!("run_one_child: starting new generation");
    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");

    let msgs_written = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let msgs_acked = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // Reader task.
    let recv_tx_reader = recv_tx.clone();
    let shared_r = shared.clone();
    let msgs_acked_r = msgs_acked.clone();
    let token_for_reader = session_token.to_string();
    let reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            if shared_r.is_shutdown_requested() {
                break;
            }
            match lines.next_line().await {
                Ok(Some(line)) => {
                    tracing::debug!(line = %line, "reader: got line");
                    msgs_acked_r.fetch_add(1, std::sync::atomic::Ordering::Release);
                    // Try to parse as OverlayEvent (with token) first
                    if let Ok(event) = serde_json::from_str::<OverlayEvent>(&line) {
                        if !validate_token(&event, &token_for_reader) {
                            tracing::warn!(
                                expected_prefix = &token_for_reader[..8.min(token_for_reader.len())],
                                "overlay event rejected: token mismatch"
                            );
                            continue;
                        }
                        match &event.command {
                            OverlayIpcCommand::Pong => {
                                let _ = recv_tx_reader.send(OverlayIpcCommand::Pong);
                            }
                            OverlayIpcCommand::Echo { payload } => {
                                let _ = recv_tx_reader.send(OverlayIpcCommand::Echo {
                                    payload: payload.clone(),
                                });
                            }
                            OverlayIpcCommand::RequestSync => {
                                let _ = recv_tx_reader.send(OverlayIpcCommand::RequestSync);
                            }
                        }
                        continue;
                    }
                    // Fallback: try legacy format (no token wrapper)
                    match decode_ndjson(&line) {
                        Ok(OverlayMessage::Ping) => {
                            let _ = recv_tx_reader.send(OverlayIpcCommand::Pong);
                        }
                        Ok(_) => {
                            tracing::warn!(line = %line, "overlay sent message on reverse pipe");
                        }
                        Err(_) => match serde_json::from_str::<OverlayIpcCommand>(&line) {
                            Ok(cmd) => {
                                // Legacy command without token — reject if token is required
                                if !token_for_reader.is_empty() {
                                    tracing::warn!("overlay event rejected: no token field");
                                    continue;
                                }
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
                return true;
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

    let status = loop {
        if shared.is_shutdown_requested() {
            drop(stdin);
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
                    None => {
                        drop(stdin);
                        break (&mut wait_fut).await;
                    }
                }
            }
        }
    };

    let clean_exit = matches!(&status, Ok(s) if s.success());
    let _ = reader.await;

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
    let session_token = opts.session_token.clone();

    loop {
        let (rx_back, carryover_out, clean_exit) = run_one_child(
            current_child,
            shared.clone(),
            send_rx,
            recv_tx.clone(),
            carryover.take(),
            &session_token,
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
            let mut drained: u64 = 0;
            while send_rx.try_recv().is_ok() {
                drained += 1;
            }
            if drained > 0 {
                tracing::warn!(drained, "drained pending messages after cap exhaustion");
            }
            drop(recv_tx);
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
    use std::fs;

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

    #[test]
    fn generate_session_token_is_64_hex_chars() {
        let token = generate_session_token();
        assert_eq!(token.len(), SESSION_TOKEN_HEX_LEN);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_session_token_is_unique() {
        let t1 = generate_session_token();
        let t2 = generate_session_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn resolve_overlay_path_returns_default_when_no_env() {
        // Clear any env vars that might interfere
        std::env::remove_var("BLUEY_OVERLAY_BIN");
        std::env::remove_var("CUE_OVERLAY_BIN");
        let default = PathBuf::from("/usr/local/bin/cue-overlay");
        let result = resolve_overlay_path(&default);
        assert_eq!(result, default);
    }

    #[test]
    fn verify_overlay_binary_rejects_relative_path() {
        let result = verify_overlay_binary(
            Path::new("relative/path/overlay"),
            Path::new("/tmp"),
        );
        assert!(matches!(result, Err(OverlayVerifyError::NotAbsolute(_))));
    }

    #[test]
    fn verify_overlay_binary_accepts_path_inside_install_dir() {
        let dir = std::env::temp_dir().join("cue_test_verify");
        let _ = fs::create_dir_all(&dir);
        let binary = dir.join("overlay");
        fs::write(&binary, b"fake").unwrap();
        let result = verify_overlay_binary(&binary, &dir);
        assert!(result.is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_overlay_binary_rejects_path_outside_install_dir() {
        let dir = std::env::temp_dir().join("cue_test_inside");
        let outside = std::env::temp_dir().join("cue_test_outside");
        let _ = fs::create_dir_all(&dir);
        let _ = fs::create_dir_all(&outside);
        let binary = outside.join("evil");
        fs::write(&binary, b"evil").unwrap();
        let result = verify_overlay_binary(&binary, &dir);
        assert!(matches!(result, Err(OverlayVerifyError::OutsideInstallDir { .. })));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }
}
