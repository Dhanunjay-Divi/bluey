//! Daemon ↔ native-overlay process IPC.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use cue_core::overlay_ipc::{
    decode_ndjson, encode_ndjson, OverlayEvent, OverlayIpcCommand, OverlayMessage,
};
use parking_lot::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;

pub const MAX_RESTART_ATTEMPTS: u32 = 5;
/// Maximum daemon-to-overlay messages waiting to be written to child stdin.
pub const OVERLAY_OUTBOUND_CHANNEL_CAPACITY: usize = 64;
/// Maximum lossless state/final messages waiting behind helper I/O.
pub const OVERLAY_CONTROL_CHANNEL_CAPACITY: usize = 32;
/// Maximum privileged overlay commands waiting for the daemon to consume them.
pub const OVERLAY_COMMAND_CHANNEL_CAPACITY: usize = 32;

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
/// overrides are ignored. In debug builds the override is accepted so local QA
/// can point at an uninstalled helper.
pub fn resolve_overlay_path(default: &Path) -> PathBuf {
    if let Some(p) = dev_overlay_override_path() {
        tracing::info!(path = %p.display(), "using overlay binary override (dev mode)");
        return p;
    }
    if !is_dev_mode() {
        if let Some(p) = env_overlay_bin() {
            tracing::warn!(
                path = %p.display(),
                "overlay binary override set but ignored in production build"
            );
        }
    }
    default.to_path_buf()
}

/// Return a configured helper override only in a debug build.
///
/// Callers use this before default helper discovery so an isolated test helper
/// does not require an installed production overlay to already exist.
pub fn dev_overlay_override_path() -> Option<PathBuf> {
    if is_dev_mode() {
        env_overlay_bin()
    } else {
        None
    }
}

/// Public alias for the dev-overlay gate so the production daemon path
/// can decide whether to allow path overrides outside the install dir.
pub fn is_dev_overlay_enabled() -> bool {
    is_dev_mode()
}

/// Returns true when we should allow env-var overrides.
fn is_dev_mode() -> bool {
    cfg!(debug_assertions)
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
    #[error(
        "overlay binary is outside install directory: binary={binary}, install_dir={install_dir}"
    )]
    OutsideInstallDir {
        binary: PathBuf,
        install_dir: PathBuf,
    },
    #[error("overlay sha256 sidecar could not be read: {path}: {source}")]
    HashSidecarReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("overlay sha256 sidecar has no 64-character hash: {0}")]
    HashSidecarInvalid(PathBuf),
    #[error("overlay binary hash mismatch: binary={binary}, expected={expected}, actual={actual}")]
    HashMismatch {
        binary: PathBuf,
        expected: String,
        actual: String,
    },
}

/// Verify that the overlay binary path is canonical and resides inside
/// `install_dir`. This prevents symlink/traversal attacks that could trick
/// the daemon into spawning an attacker-controlled binary. If a sha256
/// sidecar exists next to the helper, verify it too; release artifacts should
/// ship sidecars, while local/dev builds can still run without one.
///
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
    verify_optional_sha256_sidecar(&canonical)?;
    Ok(())
}

fn verify_optional_sha256_sidecar(path: &Path) -> Result<(), OverlayVerifyError> {
    let Some(sidecar) = sha256_sidecar_for(path) else {
        return Ok(());
    };
    let expected = read_sha256_sidecar(&sidecar)?;
    let actual =
        sha256_file_hex(path).map_err(|source| OverlayVerifyError::HashSidecarReadFailed {
            path: path.to_path_buf(),
            source,
        })?;
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(OverlayVerifyError::HashMismatch {
            binary: path.to_path_buf(),
            expected,
            actual,
        });
    }
    Ok(())
}

fn sha256_sidecar_for(path: &Path) -> Option<PathBuf> {
    let mut candidates = vec![path.with_extension("sha256")];
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        candidates.push(path.with_file_name(format!("{file_name}.sha256")));
    }
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn read_sha256_sidecar(path: &Path) -> Result<String, OverlayVerifyError> {
    let raw = std::fs::read_to_string(path).map_err(|source| {
        OverlayVerifyError::HashSidecarReadFailed {
            path: path.to_path_buf(),
            source,
        }
    })?;
    raw.split_whitespace()
        .find(|token| token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()))
        .map(|token| token.to_ascii_lowercase())
        .ok_or_else(|| OverlayVerifyError::HashSidecarInvalid(path.to_path_buf()))
}

fn sha256_file_hex(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

// ─── Item 3: Session token generation ───────────────────────────────────────

/// Generate a cryptographically random 32-byte hex session token (64 hex chars).
///
/// Uses `getrandom` to draw 256 bits of entropy from the OS source
/// (`/dev/urandom` on Linux/macOS, `BCryptGenRandom` on Windows).
///
/// Previously this concatenated two `Uuid::new_v4()` values, which gives
/// 244 bits of randomness (each UUIDv4 has 122 random bits — the other
/// 6 are version/variant). 244 bits is well past the security threshold,
/// but the doc/comment claimed "32 bytes" so codex flagged the discrepancy
/// in the R11 chain review. R12 fixes it: now the bytes are *actually*
/// 256 random bits, drawn directly from the OS entropy pool.
pub fn generate_session_token() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)?;
    // Format as lowercase hex without pulling in the `hex` crate.
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for b in bytes {
        write!(&mut out, "{b:02x}").expect("writing to String cannot fail");
    }
    Ok(out)
}

// ─── Core overlay handle ────────────────────────────────────────────────────

struct Shared {
    state: Mutex<OverlayProcessState>,
    ui_state: Mutex<cue_core::overlay_ipc::OverlayUiState>,
    shutdown_requested: std::sync::atomic::AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            state: Mutex::new(OverlayProcessState::Idle),
            ui_state: Mutex::new(cue_core::overlay_ipc::OverlayUiState::Idle),
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
    lossy_send_tx: Sender<OverlayMessage>,
    control_send_tx: Sender<OverlayMessage>,
    recv_rx: Receiver<OverlayIpcCommand>,
    shared: Arc<Shared>,
    _tasks: Vec<JoinHandle<()>>,
}

impl NativeOverlayHandle {
    pub async fn spawn(opts: OverlaySpawnOptions) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::new());
        let (lossy_send_tx, lossy_send_rx) =
            channel::<OverlayMessage>(OVERLAY_OUTBOUND_CHANNEL_CAPACITY);
        let (control_send_tx, control_send_rx) =
            channel::<OverlayMessage>(OVERLAY_CONTROL_CHANNEL_CAPACITY);
        let (recv_tx, recv_rx) = channel::<OverlayIpcCommand>(OVERLAY_COMMAND_CHANNEL_CAPACITY);

        shared.set_state(OverlayProcessState::Starting);
        let child = spawn_child(&opts).await?;
        shared.set_state(OverlayProcessState::Running);

        let tasks = wire_child(
            child,
            opts,
            shared.clone(),
            lossy_send_rx,
            control_send_rx,
            recv_tx,
        );
        Ok(Self {
            lossy_send_tx,
            control_send_tx,
            recv_rx,
            shared,
            _tasks: tasks,
        })
    }

    pub fn send(&self, msg: OverlayMessage) -> Result<(), OverlayMessage> {
        let sender = if overlay_message_is_lossy(&msg) {
            &self.lossy_send_tx
        } else {
            &self.control_send_tx
        };
        sender.try_send(msg).map_err(|e| e.into_inner())
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

    /// Current overlay UI state for event-validation gating.
    pub fn ui_state(&self) -> cue_core::overlay_ipc::OverlayUiState {
        *self.shared.ui_state.lock()
    }

    /// Update the overlay UI state. Daemon-side state machine should call
    /// this whenever the user opens/closes attach or instructions UI.
    pub fn set_ui_state(&self, state: cue_core::overlay_ipc::OverlayUiState) {
        *self.shared.ui_state.lock() = state;
    }

    pub async fn shutdown(mut self) {
        self.shared
            .shutdown_requested
            .store(true, std::sync::atomic::Ordering::Release);
        self.shared.set_state(OverlayProcessState::ShuttingDown);
        // Drop both senders so the supervisor receivers return None,
        // which triggers stdin close → child sees EOF → exits cleanly.
        let (dead_lossy_tx, _dead_lossy_rx) = channel(1);
        let (dead_control_tx, _dead_control_rx) = channel(1);
        let _ = std::mem::replace(&mut self.lossy_send_tx, dead_lossy_tx);
        let _ = std::mem::replace(&mut self.control_send_tx, dead_control_tx);
        let tasks = std::mem::take(&mut self._tasks);
        for handle in tasks {
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }
    }
}

fn overlay_message_is_lossy(message: &OverlayMessage) -> bool {
    matches!(message, OverlayMessage::TranscriptPartial { .. })
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
    apply_minimal_overlay_env(&mut cmd);
    cmd.args(&opts.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Item 3: pass session token via env var
    if !opts.session_token.is_empty() {
        cmd.env("BLUEY_OVERLAY_SESSION_TOKEN", &opts.session_token);
    }
    cmd.spawn()
}

fn apply_minimal_overlay_env(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    const ALLOWLIST: &[&str] = &[
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "ProgramData",
        "PATH",
    ];
    #[cfg(not(target_os = "windows"))]
    const ALLOWLIST: &[&str] = &[
        "HOME", "TMPDIR", "PATH", "LANG", "LC_ALL", "LC_CTYPE", "USER", "LOGNAME",
    ];

    let inherited = ALLOWLIST
        .iter()
        .filter_map(|key| std::env::var_os(key).map(|value| ((*key).to_string(), value)))
        .collect::<Vec<_>>();
    cmd.env_clear();
    for (key, value) in inherited {
        cmd.env(key, value);
    }
}

fn wire_child(
    initial_child: Child,
    opts: OverlaySpawnOptions,
    shared: Arc<Shared>,
    lossy_send_rx: Receiver<OverlayMessage>,
    control_send_rx: Receiver<OverlayMessage>,
    recv_tx: Sender<OverlayIpcCommand>,
) -> Vec<JoinHandle<()>> {
    vec![tokio::spawn(run_supervisor(
        initial_child,
        opts,
        shared,
        lossy_send_rx,
        control_send_rx,
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
    mut lossy_send_rx: Receiver<OverlayMessage>,
    mut control_send_rx: Receiver<OverlayMessage>,
    recv_tx: Sender<OverlayIpcCommand>,
    carryover_in: Option<OverlayMessage>,
    session_token: &str,
) -> (
    Receiver<OverlayMessage>,
    Receiver<OverlayMessage>,
    Option<OverlayMessage>,
    bool,
) {
    tracing::debug!("run_one_child: starting new generation");
    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take();

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
                    tracing::debug!(line_bytes = line.len(), "reader: got overlay line");
                    msgs_acked_r.fetch_add(1, std::sync::atomic::Ordering::Release);
                    // Try to parse as OverlayEvent (with token) first
                    if let Ok(event) = serde_json::from_str::<OverlayEvent>(&line) {
                        if !validate_token(&event, &token_for_reader) {
                            tracing::warn!("overlay event rejected: token mismatch");
                            continue;
                        }
                        if recv_tx_reader.send(event.command).await.is_err() {
                            break;
                        }
                        continue;
                    }
                    // Fallback: try legacy format (no token wrapper)
                    match decode_ndjson(&line) {
                        Ok(OverlayMessage::Ping) => {
                            if recv_tx_reader.send(OverlayIpcCommand::Pong).await.is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            tracing::warn!(
                                line_bytes = line.len(),
                                "overlay sent unsupported message on reverse pipe"
                            );
                        }
                        Err(_) => match serde_json::from_str::<OverlayIpcCommand>(&line) {
                            Ok(cmd) => {
                                // Legacy command without token — reject if token is required
                                if !token_for_reader.is_empty() {
                                    tracing::warn!("overlay event rejected: no token field");
                                    continue;
                                }
                                if recv_tx_reader.send(cmd).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    line_bytes = line.len(),
                                    "bad overlay stdout line"
                                );
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
    let stderr_reader = stderr.map(|mut stderr| {
        tokio::spawn(async move {
            let mut buffer = [0u8; 4096];
            let mut total_bytes = 0u64;
            loop {
                match stderr.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(read) => total_bytes = total_bytes.saturating_add(read as u64),
                    Err(error) => {
                        tracing::warn!(
                            error_kind = ?error.kind(),
                            "overlay stderr drain failed"
                        );
                        break;
                    }
                }
            }
            if total_bytes > 0 {
                tracing::debug!(total_bytes, "overlay emitted redacted stderr diagnostics");
            }
        })
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
            return (lossy_send_rx, control_send_rx, carryover_in, clean);
        }
        msgs_written.fetch_add(1, std::sync::atomic::Ordering::Release);
    }

    // Main select loop.
    let mut wait_fut = Box::pin(child.wait());
    let mut last_msg: Option<OverlayMessage> = carryover_in;
    let mut write_failed = false;
    let mut lossy_open = true;
    let mut control_open = true;

    let status = loop {
        if shared.is_shutdown_requested() {
            // Drop stdin so child sees EOF and exits cleanly.
            drop(stdin);
            break (&mut wait_fut).await;
        }
        tokio::select! {
            biased;
            s = &mut wait_fut => break s,
            msg = control_send_rx.recv(), if control_open => {
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
                        control_open = false;
                    }
                }
            }
            msg = lossy_send_rx.recv(), if lossy_open => {
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
                        lossy_open = false;
                    }
                }
            }
        }
        if !lossy_open && !control_open {
            // Both channels closed (shutdown). Close stdin so child exits.
            drop(stdin);
            break (&mut wait_fut).await;
        }
    };

    let clean_exit = matches!(&status, Ok(s) if s.success());

    // Ensure reader task completes.
    let _ = reader.await;
    if let Some(stderr_reader) = stderr_reader {
        let _ = stderr_reader.await;
    }

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
    (lossy_send_rx, control_send_rx, carryover_out, clean_exit)
}

async fn run_supervisor(
    initial_child: Child,
    opts: OverlaySpawnOptions,
    shared: Arc<Shared>,
    mut lossy_send_rx: Receiver<OverlayMessage>,
    mut control_send_rx: Receiver<OverlayMessage>,
    recv_tx: Sender<OverlayIpcCommand>,
) {
    let mut current_child = initial_child;
    let mut consecutive_failures: u32 = 0;
    let mut carryover: Option<OverlayMessage> = None;
    let session_token = opts.session_token.clone();

    loop {
        let (lossy_rx_back, control_rx_back, carryover_out, clean_exit) = run_one_child(
            current_child,
            shared.clone(),
            lossy_send_rx,
            control_send_rx,
            recv_tx.clone(),
            carryover.take(),
            &session_token,
        )
        .await;
        lossy_send_rx = lossy_rx_back;
        control_send_rx = control_rx_back;
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
            // Drain pending messages so senders see backpressure immediately.
            let mut drained: u64 = 0;
            while lossy_send_rx.try_recv().is_ok() {
                drained += 1;
            }
            while control_send_rx.try_recv().is_ok() {
                drained += 1;
            }
            if drained > 0 {
                tracing::warn!(drained, "drained pending messages after cap exhaustion");
            }
            // Drop recv_tx so downstream readers see channel close.
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

    #[test]
    fn partial_saturation_does_not_drop_terminal_event() {
        let (lossy_send_tx, lossy_send_rx) = channel(OVERLAY_OUTBOUND_CHANNEL_CAPACITY);
        let (control_send_tx, mut control_send_rx) = channel(OVERLAY_CONTROL_CHANNEL_CAPACITY);
        let (_recv_tx, recv_rx) = channel(1);
        let handle = NativeOverlayHandle {
            lossy_send_tx,
            control_send_tx,
            recv_rx,
            shared: Arc::new(Shared::new()),
            _tasks: Vec::new(),
        };

        for _ in 0..OVERLAY_OUTBOUND_CHANNEL_CAPACITY {
            handle
                .send(OverlayMessage::TranscriptPartial {
                    source: "mic".into(),
                    text: "delta".into(),
                })
                .expect("queue has space");
        }
        let terminal = OverlayMessage::TranscriptFinal {
            source: "mic".into(),
            text: "complete".into(),
        };
        handle
            .send(terminal.clone())
            .expect("terminal uses a separate lossless lane");
        assert_eq!(lossy_send_rx.len(), OVERLAY_OUTBOUND_CHANNEL_CAPACITY);
        assert_eq!(control_send_rx.try_recv().unwrap(), terminal);
    }

    #[tokio::test]
    async fn privileged_command_waits_for_capacity_instead_of_dropping() {
        let (sender, mut receiver) = channel(1);
        sender.send(OverlayIpcCommand::Pong).await.unwrap();

        let blocked = tokio::spawn(async move {
            sender.send(OverlayIpcCommand::RequestSync).await.unwrap();
        });
        tokio::task::yield_now().await;
        assert!(!blocked.is_finished());

        assert_eq!(receiver.recv().await, Some(OverlayIpcCommand::Pong));
        blocked.await.unwrap();
        assert_eq!(receiver.recv().await, Some(OverlayIpcCommand::RequestSync));
    }

    #[test]
    fn token_is_64_hex_chars() {
        let t = generate_session_token().expect("generate token");
        assert_eq!(t.len(), 64, "expected 64 hex chars (32 bytes)");
        assert!(
            t.chars()
                .all(|c| c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_lowercase())),
            "token must be lowercase hex: {t}"
        );
    }

    #[test]
    fn token_is_unique_across_calls() {
        // 1000 fresh tokens — collision probability is negligible at 256 bits
        // (and any collision indicates a serious entropy bug).
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let t = generate_session_token().expect("generate token");
            assert!(seen.insert(t), "duplicate token within 1000 calls");
        }
    }

    #[test]
    fn token_has_no_prefix_pattern_from_old_uuid_impl() {
        // The old UUID-based impl always set bits 6-7 of byte 6 to specific
        // values (UUID variant) and bits 12-15 of byte 6 to 4 (UUID version).
        // The new impl draws from getrandom, so the 7th hex char (= upper
        // nibble of byte 6) should NOT be biased toward 4.
        // Sample 200 tokens and assert variety in that position.
        let mut seventh_char_set = std::collections::HashSet::new();
        for _ in 0..200 {
            let t = generate_session_token().expect("generate token");
            seventh_char_set.insert(t.chars().nth(12).unwrap());
        }
        // With true randomness across 200 samples we should see >= 8 distinct
        // hex digits in any single position.
        assert!(
            seventh_char_set.len() >= 8,
            "position 12 seems biased: only {} distinct values",
            seventh_char_set.len()
        );
    }
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
        let token = generate_session_token().expect("generate token");
        assert_eq!(token.len(), SESSION_TOKEN_HEX_LEN);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_session_token_is_unique() {
        let t1 = generate_session_token().expect("generate token");
        let t2 = generate_session_token().expect("generate token");
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
    fn debug_overlay_override_does_not_require_a_discoverable_default() {
        let override_path = PathBuf::from("/tmp/bluey-test-overlay");
        std::env::set_var("BLUEY_OVERLAY_BIN", &override_path);
        std::env::remove_var("CUE_OVERLAY_BIN");
        let result = dev_overlay_override_path();
        std::env::remove_var("BLUEY_OVERLAY_BIN");

        if cfg!(debug_assertions) {
            assert_eq!(result, Some(override_path));
        } else {
            assert_eq!(result, None);
        }
    }

    #[test]
    fn verify_overlay_binary_rejects_relative_path() {
        let result = verify_overlay_binary(Path::new("relative/path/overlay"), Path::new("/tmp"));
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
    fn verify_overlay_binary_accepts_matching_sha256_sidecar() {
        let dir =
            std::env::temp_dir().join(format!("cue_test_verify_hash_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&dir);
        let binary = dir.join("overlay");
        fs::write(&binary, b"fake-overlay").unwrap();
        let expected = sha256_file_hex(&binary).unwrap();
        fs::write(
            binary.with_file_name("overlay.sha256"),
            format!("{expected}  overlay\n"),
        )
        .unwrap();

        let result = verify_overlay_binary(&binary, &dir);
        assert!(result.is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_overlay_binary_rejects_mismatched_sha256_sidecar() {
        let dir =
            std::env::temp_dir().join(format!("cue_test_verify_hash_bad_{}", uuid::Uuid::new_v4()));
        let _ = fs::create_dir_all(&dir);
        let binary = dir.join("overlay");
        fs::write(&binary, b"fake-overlay").unwrap();
        fs::write(
            binary.with_file_name("overlay.sha256"),
            format!("{}  overlay\n", "0".repeat(64)),
        )
        .unwrap();

        let result = verify_overlay_binary(&binary, &dir);
        assert!(matches!(
            result,
            Err(OverlayVerifyError::HashMismatch { .. })
        ));
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
        assert!(matches!(
            result,
            Err(OverlayVerifyError::OutsideInstallDir { .. })
        ));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }
}
