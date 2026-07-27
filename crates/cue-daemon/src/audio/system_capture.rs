//! System audio capture via native OS helpers.
//!
//! Spawns the platform-specific native audio helper as a child process in
//! `--continuous` mode. The helper streams 16 kHz mono i16 LE PCM on stdout.
//! This module reads that stream, frames it into 20 ms `AudioChunk`s with
//! `source: System`, and pushes them to the provided channel.
//!
//! Restart-on-crash with exponential backoff mirrors the overlay pattern.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

/// Samples per 20 ms chunk at 16 kHz mono.
const CHUNK_SAMPLES: usize = 320;
/// Bytes per chunk: 320 samples * 2 bytes each.
const CHUNK_BYTES: usize = CHUNK_SAMPLES * 2;
/// Maximum consecutive restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 5;
/// Keep helper diagnostics bounded while continuing to drain stderr so the
/// child cannot block on a full pipe.
const MAX_HELPER_STDERR_BYTES: usize = 16 * 1024;

/// Handle to a running system audio capture session.
pub struct SystemAudioCapture {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
    status: CaptureStatusHandle,
}

/// Observable lifecycle of a native audio helper.
///
/// Spawning the supervisor is not proof that macOS granted access or that PCM
/// is flowing. The daemon observes this state so it can keep the overlay on
/// "connecting" until the first bytes arrive and report terminal failures
/// instead of leaving a stale "listening" indicator behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CaptureStatus {
    Starting = 0,
    Running = 1,
    PermissionDenied = 2,
    Failed = 3,
    Stopped = 4,
}

#[derive(Clone)]
pub struct CaptureStatusHandle {
    value: Arc<AtomicU8>,
}

impl CaptureStatusHandle {
    fn new(status: CaptureStatus) -> Self {
        Self {
            value: Arc::new(AtomicU8::new(status as u8)),
        }
    }

    fn set(&self, status: CaptureStatus) {
        self.value.store(status as u8, Ordering::Release);
    }

    pub fn get(&self) -> CaptureStatus {
        match self.value.load(Ordering::Acquire) {
            0 => CaptureStatus::Starting,
            1 => CaptureStatus::Running,
            2 => CaptureStatus::PermissionDenied,
            3 => CaptureStatus::Failed,
            _ => CaptureStatus::Stopped,
        }
    }

    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}

/// Which capture the native helper runs — selects the helper's `--source` arg
/// and the `AudioChunk.source` tag. Microphone mode enables Apple's
/// VoiceProcessingIO acoustic echo cancellation in the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureRole {
    System,
    Microphone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureSessionOutcome {
    Clean,
    RetryableFailure,
    PermissionDenied,
    NonRetryableFailure,
}

impl CaptureRole {
    fn audio_source(self) -> AudioSource {
        match self {
            CaptureRole::System => AudioSource::System,
            CaptureRole::Microphone => AudioSource::Microphone,
        }
    }
}

impl SystemAudioCapture {
    /// Start system audio capture (whole-display). Spawns the native helper and
    /// streams `AudioChunk`s to `sender`.
    pub fn start(sender: UnboundedSender<AudioChunk>) -> std::io::Result<Self> {
        Self::start_with_mode(sender, false)
    }

    /// Start system audio capture, optionally via the interactive picker
    /// (`pick = true`): the helper presents the macOS content-sharing picker so
    /// the user chooses which app to capture, then streams that app's audio.
    /// PCM output is identical to whole-display mode, so the pipeline is the same.
    pub fn start_with_mode(
        sender: UnboundedSender<AudioChunk>,
        pick: bool,
    ) -> std::io::Result<Self> {
        let binary = resolve_binary()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let status = CaptureStatusHandle::new(CaptureStatus::Starting);
        let task_status = status.clone();

        let task = tokio::spawn(async move {
            supervisor_loop(
                binary,
                sender,
                stop_clone,
                pick,
                CaptureRole::System,
                task_status,
            )
            .await;
        });

        Ok(Self {
            stop,
            task: Some(task),
            status,
        })
    }

    /// Start MICROPHONE capture via the SAME native helper in `--source
    /// microphone` mode. Critically this enables Apple's VoiceProcessingIO
    /// acoustic echo cancellation (see the Swift helper), so the far side's voice
    /// leaking from the speakers into the mic is removed BEFORE STT — the fix for
    /// the mic double-transcribing system audio when the user is on speakers (no
    /// headphones). PCM output is the identical 16 kHz mono i16 stream; only the
    /// `AudioChunk.source` is `Microphone`.
    ///
    /// Independent of the system-audio tap permission-wise: VoiceProcessingIO is a
    /// plain microphone unit and needs only the Microphone TCC grant, not Screen /
    /// System-Audio Recording — so running it alongside the system tap does not
    /// contend for the same grant.
    pub fn start_microphone(sender: UnboundedSender<AudioChunk>) -> std::io::Result<Self> {
        let binary = resolve_binary()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let status = CaptureStatusHandle::new(CaptureStatus::Starting);
        let task_status = status.clone();

        let task = tokio::spawn(async move {
            supervisor_loop(
                binary,
                sender,
                stop_clone,
                false,
                CaptureRole::Microphone,
                task_status,
            )
            .await;
        });

        Ok(Self {
            stop,
            task: Some(task),
            status,
        })
    }

    pub fn status(&self) -> CaptureStatus {
        self.status.get()
    }

    pub fn status_handle(&self) -> CaptureStatusHandle {
        self.status.clone()
    }

    /// True while the supervisor is starting or actively receiving PCM.
    pub fn is_active(&self) -> bool {
        matches!(
            self.status(),
            CaptureStatus::Starting | CaptureStatus::Running
        ) && self.task.as_ref().is_some_and(|task| !task.is_finished())
    }

    /// Signal the capture to stop and wait for the task to finish.
    pub async fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        self.status.set(CaptureStatus::Stopped);
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(Duration::from_secs(3), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }

    /// TEST-ONLY: drive the pipeline from a 16 kHz mono WAV file instead of the
    /// native capture helper. Frames the file into the same 20 ms `AudioChunk`s and
    /// pushes them on a real-time cadence, so the whole downstream pipeline
    /// (retention → STT → diarization) runs on deterministic, benchmarkable audio
    /// with no OS permission. Used by `BLUEY_AUDIO_WAV_FILE`.
    pub fn start_from_wav(
        sender: UnboundedSender<AudioChunk>,
        path: String,
    ) -> std::io::Result<Self> {
        let samples = read_wav_i16(&path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let status = CaptureStatusHandle::new(CaptureStatus::Starting);
        let task_status = status.clone();
        let task = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            let mut base_ms: u64 = 0;
            task_status.set(CaptureStatus::Running);
            for frame in samples.chunks(CHUNK_SAMPLES) {
                if stop_clone.load(Ordering::Acquire) {
                    break;
                }
                tick.tick().await;
                let chunk = AudioChunk {
                    source: AudioSource::System,
                    sample_rate: SampleRate::SR_16K,
                    samples: frame.to_vec(),
                    captured_at_ms: epoch_ms().saturating_add(base_ms),
                };
                base_ms = base_ms.saturating_add(20);
                if sender.send(chunk).is_err() {
                    break;
                }
            }
            if stop_clone.load(Ordering::Acquire) {
                task_status.set(CaptureStatus::Stopped);
            } else {
                task_status.set(CaptureStatus::Failed);
            }
        });
        Ok(Self {
            stop,
            task: Some(task),
            status,
        })
    }
}

/// Minimal 16-bit PCM WAV reader (finds the `data` chunk; assumes 16 kHz mono).
fn read_wav_i16(path: &str) -> std::io::Result<Vec<i16>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a RIFF/WAV file",
        ));
    }
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let sz = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        if id == b"data" {
            let end = (body + sz).min(bytes.len());
            let mut out = Vec::with_capacity((end - body) / 2);
            let mut i = body;
            while i + 1 < end {
                out.push(i16::from_le_bytes([bytes[i], bytes[i + 1]]));
                i += 2;
            }
            return Ok(out);
        }
        pos = body + sz + (sz & 1);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "no data chunk in WAV",
    ))
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.status.set(CaptureStatus::Stopped);
    }
}

fn resolve_binary() -> std::io::Result<PathBuf> {
    // Allow override for testing
    if let Ok(path) = std::env::var("BLUEY_SYSTEM_AUDIO_BINARY") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    let path = platform_binary_path();
    if path.exists() {
        return Ok(path);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("system audio binary not found at {}", path.display()),
    ))
}

#[cfg(target_os = "macos")]
fn platform_binary_path() -> PathBuf {
    // PREFER the .app bundle's inner binary. ScreenCaptureKit system-audio
    // capture is TCC-gated, and a bare CLI binary can NEVER hold the Screen
    // Recording grant (it doesn't appear in System Settings, so the user can't
    // enable it, and capture returns silent buffers). The `BlueyAudio.app` bundle
    // CAN be granted, and once granted its inner binary captures real audio. So
    // we look for the bundle first, falling back to the bare binary only for old
    // installs / dev builds without the bundle.
    let app_inner = "BlueyAudio.app/Contents/MacOS/BlueyAudio";
    if let Ok(exe) = std::env::current_exe() {
        let mut dirs = Vec::new();
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
        if let Ok(canonical) = exe.canonicalize() {
            if let Some(dir) = canonical.parent() {
                dirs.push(dir.to_path_buf());
            }
        }
        for dir in dirs {
            for candidate in [
                dir.join(app_inner),
                dir.join("bluey-audio-macos"),
                dir.join("cue-audio-macos"),
            ] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    // Dev fallback: the bundle built by native/macos/cue-audio/bundle-app.sh.
    let dev_app = PathBuf::from(format!("native/macos/cue-audio/.build/{app_inner}"));
    if dev_app.exists() {
        return dev_app;
    }
    PathBuf::from("native/macos/cue-audio/.build/bluey-audio-macos")
}

#[cfg(target_os = "windows")]
fn platform_binary_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let mut dirs = Vec::new();
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
        if let Ok(canonical) = exe.canonicalize() {
            if let Some(dir) = canonical.parent() {
                dirs.push(dir.to_path_buf());
            }
        }
        for dir in dirs {
            for candidate in [dir.join("bluey-audio.exe"), dir.join("cue-audio.exe")] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from("native/windows/cue-audio/build/bluey-audio.exe")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_binary_path() -> PathBuf {
    PathBuf::from("bluey-audio")
}

/// Resolve the `BlueyAudio.app` BUNDLE directory (not the inner binary) so the
/// MIC helper can be launched via `/usr/bin/open`, which reads the bundle
/// Info.plist (NSMicrophoneUsageDescription) that mic access requires. Mirrors
/// the search order of [`platform_binary_path`]. Returns `None` when only a bare
/// binary exists (dev builds without the bundle).
#[cfg(target_os = "macos")]
fn macos_app_bundle_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BLUEY_AUDIO_APP_BUNDLE") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let app = "BlueyAudio.app";
    if let Ok(exe) = std::env::current_exe() {
        let mut dirs = Vec::new();
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
        if let Ok(canonical) = exe.canonicalize() {
            if let Some(dir) = canonical.parent() {
                dirs.push(dir.to_path_buf());
            }
        }
        for dir in dirs {
            let candidate = dir.join(app);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let dev_app = PathBuf::from(format!("native/macos/cue-audio/.build/{app}"));
    if dev_app.exists() {
        return Some(dev_app);
    }
    None
}

/// Frame freshly-read bytes (already in `buf[..offset]`) into whole 20 ms
/// `AudioChunk`s tagged with `role`'s source, and push them. Returns `false` if
/// the receiver was dropped (caller should stop). `offset` is left holding the
/// leftover partial-chunk bytes.
fn drain_frames(
    buf: &mut [u8],
    offset: &mut usize,
    sender: &UnboundedSender<AudioChunk>,
    role: CaptureRole,
) -> bool {
    while *offset >= CHUNK_BYTES {
        let samples: Vec<i16> = buf[..CHUNK_BYTES]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let chunk = AudioChunk {
            source: role.audio_source(),
            sample_rate: SampleRate::SR_16K,
            samples,
            captured_at_ms: epoch_ms(),
        };
        if sender.send(chunk).is_err() {
            return false;
        }
        buf.copy_within(CHUNK_BYTES..*offset, 0);
        *offset -= CHUNK_BYTES;
    }
    true
}

async fn supervisor_loop(
    binary: PathBuf,
    sender: UnboundedSender<AudioChunk>,
    stop: Arc<AtomicBool>,
    pick: bool,
    role: CaptureRole,
    status: CaptureStatusHandle,
) {
    let mut consecutive_failures: u32 = 0;

    loop {
        if stop.load(Ordering::Acquire) {
            status.set(CaptureStatus::Stopped);
            return;
        }
        status.set(CaptureStatus::Starting);

        // The MIC helper MUST be launched as the .app bundle (macOS reads
        // NSMicrophoneUsageDescription from the bundle Info.plist — a bare exec
        // lacks it and macOS traps on mic access), so it uses the open+socket
        // transport. SYSTEM audio works fine as a bare exec over stdout.
        #[cfg(target_os = "macos")]
        let outcome = if role == CaptureRole::Microphone {
            if let Some(bundle) = macos_app_bundle_path() {
                run_mic_socket_session(&bundle, &sender, &stop, &status).await
            } else {
                // No bundle (dev build with only a bare binary) — fall back to
                // stdout. The mic will trap without the plist, but this keeps a
                // plain checkout from failing to spawn; system audio still works.
                run_stdout_session(&binary, pick, role, &sender, &stop, &status).await
            }
        } else {
            run_stdout_session(&binary, pick, role, &sender, &stop, &status).await
        };
        #[cfg(not(target_os = "macos"))]
        let outcome = run_stdout_session(&binary, pick, role, &sender, &stop, &status).await;

        if stop.load(Ordering::Acquire) {
            status.set(CaptureStatus::Stopped);
            return;
        }

        match outcome {
            CaptureSessionOutcome::PermissionDenied => {
                status.set(CaptureStatus::PermissionDenied);
                tracing::error!(
                    ?role,
                    "native audio capture permission denied; waiting for an explicit retry"
                );
                return;
            }
            CaptureSessionOutcome::NonRetryableFailure => {
                status.set(CaptureStatus::Failed);
                tracing::error!(
                    ?role,
                    "native audio helper failed during setup; waiting for an explicit retry"
                );
                return;
            }
            CaptureSessionOutcome::Clean => {
                // A continuous helper that exits without an explicit stop is no
                // longer a live capture even when it returned exit code zero.
                status.set(CaptureStatus::Failed);
                return;
            }
            CaptureSessionOutcome::RetryableFailure => {}
        }

        consecutive_failures += 1;
        if consecutive_failures > MAX_RESTART_ATTEMPTS {
            status.set(CaptureStatus::Failed);
            tracing::error!(
                ?role,
                attempts = consecutive_failures,
                "native audio helper failed too many times; giving up"
            );
            return;
        }

        let delay = restart_delay(consecutive_failures - 1);
        tracing::warn!(
            ?role,
            attempt = consecutive_failures,
            delay_ms = delay.as_millis() as u64,
            "native audio helper exited unexpectedly; respawning"
        );
        if wait_for_stop(&stop, delay).await {
            status.set(CaptureStatus::Stopped);
            return;
        }
    }
}

async fn collect_helper_stderr(mut stderr: tokio::process::ChildStderr) -> String {
    let mut captured = Vec::with_capacity(MAX_HELPER_STDERR_BYTES);
    let mut chunk = [0_u8; 1024];
    loop {
        match stderr.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => {
                let remaining = MAX_HELPER_STDERR_BYTES.saturating_sub(captured.len());
                captured.extend_from_slice(&chunk[..read.min(remaining)]);
            }
            Err(error) => {
                tracing::debug!(%error, "failed to read native audio helper stderr");
                break;
            }
        }
    }
    String::from_utf8_lossy(&captured).into_owned()
}

fn classify_helper_exit(
    status: std::io::Result<std::process::ExitStatus>,
    stderr: &str,
) -> CaptureSessionOutcome {
    match status {
        Ok(status)
            if is_system_audio_permission_denied(status)
                || is_system_audio_permission_denied_message(stderr) =>
        {
            CaptureSessionOutcome::PermissionDenied
        }
        Ok(status) if status.success() => CaptureSessionOutcome::Clean,
        Ok(_) if is_terminal_helper_failure_message(stderr) => {
            CaptureSessionOutcome::NonRetryableFailure
        }
        Ok(_) | Err(_) => CaptureSessionOutcome::RetryableFailure,
    }
}

fn is_terminal_helper_failure_message(message: &str) -> bool {
    let message = message.to_lowercase();
    [
        "bad cpu type",
        "code signature",
        "codesign",
        "dyld",
        "exec format",
        "image not found",
        "library not loaded",
        "no matching profile",
        "operation not permitted",
        "provisioning profile",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn classify_helper_spawn_error(error: &std::io::Error) -> CaptureSessionOutcome {
    use std::io::ErrorKind;

    let terminal_kind = matches!(
        error.kind(),
        ErrorKind::NotFound
            | ErrorKind::PermissionDenied
            | ErrorKind::InvalidInput
            | ErrorKind::Unsupported
    );
    let terminal_message = is_terminal_helper_failure_message(&error.to_string());

    if terminal_kind || terminal_message {
        CaptureSessionOutcome::NonRetryableFailure
    } else {
        CaptureSessionOutcome::RetryableFailure
    }
}

#[cfg(target_os = "macos")]
struct TempPathCleanup(PathBuf);

#[cfg(target_os = "macos")]
impl Drop for TempPathCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(target_os = "macos")]
fn microphone_setup_outcome(status_path: &std::path::Path) -> CaptureSessionOutcome {
    match microphone_helper_status(status_path).as_deref() {
        Some("permission_denied") => CaptureSessionOutcome::PermissionDenied,
        Some(_) | None => CaptureSessionOutcome::NonRetryableFailure,
    }
}

#[cfg(target_os = "macos")]
fn microphone_helper_status(status_path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(status_path)
        .ok()?
        .lines()
        .next()
        .map(str::to_owned)
}

#[cfg(target_os = "macos")]
fn microphone_helper_pid(status_path: &std::path::Path) -> Option<libc::pid_t> {
    let status = std::fs::read_to_string(status_path).ok()?;
    let pid = status
        .lines()
        .find_map(|line| line.strip_prefix("pid="))?
        .parse::<libc::pid_t>()
        .ok()?;
    (pid > 1 && pid != std::process::id() as libc::pid_t).then_some(pid)
}

#[cfg(target_os = "macos")]
fn terminate_microphone_helper(status_path: &std::path::Path) {
    if let Some(pid) = microphone_helper_pid(status_path) {
        // SAFETY: `pid` is a positive helper-owned process identifier read from
        // this launch's unique status handshake. SIGTERM is best-effort.
        let result = unsafe { libc::kill(pid, libc::SIGTERM) };
        if result != 0 {
            tracing::debug!(
                pid,
                error = %std::io::Error::last_os_error(),
                "mic: detached helper was already gone"
            );
        }
    }
}

async fn wait_for_stop(stop: &AtomicBool, duration: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        if stop.load(Ordering::Acquire) {
            return true;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return false;
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(100))).await;
    }
}

/// Bare-exec + stdout transport (system audio; non-macOS; macOS dev fallback).
async fn run_stdout_session(
    binary: &std::path::Path,
    pick: bool,
    role: CaptureRole,
    sender: &UnboundedSender<AudioChunk>,
    stop: &Arc<AtomicBool>,
    capture_status: &CaptureStatusHandle,
) -> CaptureSessionOutcome {
    let mut cmd = Command::new(binary);
    if pick {
        cmd.args(["--pick", "--continuous"]);
    } else {
        let source = match role {
            CaptureRole::System => "system",
            CaptureRole::Microphone => "microphone",
        };
        cmd.args(["--source", source, "--continuous"]);
    }
    let mut child = match cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "failed to spawn native audio helper");
            return classify_helper_spawn_error(&e);
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill().await;
        return CaptureSessionOutcome::NonRetryableFailure;
    };
    let stderr_task = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(collect_helper_stderr(stderr)));
    let mut buf = vec![0u8; CHUNK_BYTES.max(8192)];
    let mut offset = 0usize;
    let mut stopped_intentionally = false;
    loop {
        if stop.load(Ordering::Acquire) {
            let _ = child.kill().await;
            stopped_intentionally = true;
            break;
        }
        let read =
            tokio::time::timeout(Duration::from_millis(250), stdout.read(&mut buf[offset..])).await;
        match read {
            Ok(Ok(0)) => break, // EOF
            Ok(Ok(n)) => {
                capture_status.set(CaptureStatus::Running);
                offset += n;
                if !drain_frames(&mut buf, &mut offset, sender, role) {
                    let _ = child.kill().await;
                    stopped_intentionally = true;
                    break;
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, ?role, "failed to read native audio helper output");
                let _ = child.kill().await;
                break;
            }
            Err(_) => continue,
        }
    }
    let status = child.wait().await;
    let helper_stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    if !helper_stderr.trim().is_empty() {
        tracing::debug!(
            ?role,
            stderr = %helper_stderr.trim(),
            "native audio helper diagnostics"
        );
    }
    if stopped_intentionally {
        CaptureSessionOutcome::Clean
    } else {
        classify_helper_exit(status, &helper_stderr)
    }
}

/// macOS bundle + UNIX-socket transport for the MIC helper. Binds a socket,
/// launches `BlueyAudio.app` via `/usr/bin/open` (so macOS reads the bundle
/// Info.plist and grants mic access), and reads PCM from the accepted
/// connection. Mirrors the overlay's open+socket handshake.
#[cfg(target_os = "macos")]
async fn run_mic_socket_session(
    bundle: &std::path::Path,
    sender: &UnboundedSender<AudioChunk>,
    stop: &Arc<AtomicBool>,
    capture_status: &CaptureStatusHandle,
) -> CaptureSessionOutcome {
    use tokio::net::UnixListener;

    let session_suffix = epoch_ms();
    let socket_path = std::env::temp_dir().join(format!(
        "bluey-mic-{}-{session_suffix}.sock",
        std::process::id()
    ));
    let status_path = std::env::temp_dir().join(format!(
        "bluey-mic-{}-{session_suffix}.status",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&status_path);
    let _socket_cleanup = TempPathCleanup(socket_path.clone());
    let _status_cleanup = TempPathCleanup(status_path.clone());
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(error = %e, "mic: failed to bind capture socket");
            return CaptureSessionOutcome::RetryableFailure;
        }
    };

    // `open -n <BlueyAudio.app> --args --source microphone --socket <path>`.
    // No `-W` (that would block until the app exits); open returns after launch.
    let mut cmd = Command::new("/usr/bin/open");
    cmd.arg("-n")
        .arg(bundle)
        .arg("--args")
        .args(["--source", "microphone", "--continuous"])
        .arg("--socket")
        .arg(&socket_path)
        .arg("--status-file")
        .arg(&status_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    tracing::info!(
        bundle = %bundle.display(),
        socket = %socket_path.display(),
        "mic: launching BlueyAudio.app (open -n --source microphone)"
    );
    if !matches!(cmd.status().await, Ok(s) if s.success()) {
        tracing::warn!("mic: `open` failed to launch BlueyAudio.app");
        let _ = std::fs::remove_file(&socket_path);
        return CaptureSessionOutcome::NonRetryableFailure;
    }
    tracing::info!("mic: `open` returned OK; waiting for helper to connect to the socket…");

    let accept_deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let (mut conn, _addr) = loop {
        if stop.load(Ordering::Acquire) {
            terminate_microphone_helper(&status_path);
            return CaptureSessionOutcome::Clean;
        }
        if tokio::time::Instant::now() >= accept_deadline {
            tracing::warn!(
                "mic: helper did not connect before timeout — BlueyAudio.app likely \
                 crashed on launch (missing NSMicrophoneUsageDescription?) or was denied"
            );
            terminate_microphone_helper(&status_path);
            return microphone_setup_outcome(&status_path);
        }
        match tokio::time::timeout(Duration::from_millis(250), listener.accept()).await {
            Ok(Ok(pair)) => break pair,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "mic: socket accept failed");
                terminate_microphone_helper(&status_path);
                return CaptureSessionOutcome::RetryableFailure;
            }
            Err(_) => {}
        }
    };
    tracing::info!("mic: helper connected — reading audio frames");

    let mut buf = vec![0u8; CHUNK_BYTES.max(8192)];
    let mut offset = 0usize;
    // Debug telemetry: total bytes read + a rough peak amplitude so the log shows
    // whether audio is FLOWING and whether it's REAL vs SILENCE. macOS delivers a
    // connected-but-silent stream when the Microphone grant is missing (the
    // helper runs, frames arrive, but every sample is ~0) — this distinguishes
    // "no permission" (bytes flow, peak ≈ 0) from "helper dead" (no bytes).
    let mut total_bytes: u64 = 0;
    let mut peak_abs: i32 = 0;
    let mut last_report = std::time::Instant::now();
    let mut reported_first_audio = false;
    let mut microphone_authorized_at = None;
    let outcome = loop {
        if stop.load(Ordering::Acquire) {
            terminate_microphone_helper(&status_path);
            break CaptureSessionOutcome::Clean;
        }
        // Short read timeout so the stop flag is checked promptly even when the
        // mic is silent.
        let read =
            tokio::time::timeout(Duration::from_millis(250), conn.read(&mut buf[offset..])).await;
        match read {
            Ok(Ok(0)) => {
                tracing::warn!(
                    total_bytes,
                    "mic: helper closed the stream (EOF) — it exited"
                );
                break if total_bytes == 0 {
                    microphone_setup_outcome(&status_path)
                } else {
                    CaptureSessionOutcome::RetryableFailure
                };
            } // EOF: helper exited
            Ok(Ok(n)) => {
                capture_status.set(CaptureStatus::Running);
                total_bytes += n as u64;
                // Sample peak amplitude over the freshly-read bytes (i16 LE PCM).
                let fresh = &buf[offset..offset + n];
                for pair in fresh.chunks_exact(2) {
                    let s = i16::from_le_bytes([pair[0], pair[1]]) as i32;
                    let a = s.abs();
                    if a > peak_abs {
                        peak_abs = a;
                    }
                }
                if !reported_first_audio {
                    tracing::info!(bytes = n, "mic: first audio bytes received");
                    reported_first_audio = true;
                }
                // Report ~every 3s: bytes/s and peak. peak ≈ 0 over seconds ⇒
                // connected but SILENT ⇒ Microphone permission almost certainly
                // not granted to sh.bluey.audio.
                if last_report.elapsed() >= Duration::from_secs(3) {
                    if peak_abs < 8 {
                        tracing::warn!(
                            total_bytes,
                            peak_abs,
                            "mic: audio is flowing but SILENT (peak ~0) — grant \
                             Microphone to BlueyAudio in System Settings → Privacy"
                        );
                    } else {
                        tracing::info!(total_bytes, peak_abs, "mic: audio flowing (real signal)");
                    }
                    peak_abs = 0;
                    last_report = std::time::Instant::now();
                }
                offset += n;
                if !drain_frames(&mut buf, &mut offset, sender, CaptureRole::Microphone) {
                    break CaptureSessionOutcome::Clean; // receiver dropped
                }
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "mic: failed to read helper stream");
                break if total_bytes == 0 {
                    microphone_setup_outcome(&status_path)
                } else {
                    CaptureSessionOutcome::RetryableFailure
                };
            }
            Err(_) if total_bytes == 0 => {
                match microphone_helper_status(&status_path).as_deref() {
                    Some("permission_denied") => {
                        terminate_microphone_helper(&status_path);
                        break CaptureSessionOutcome::PermissionDenied;
                    }
                    Some("failed") => {
                        terminate_microphone_helper(&status_path);
                        break CaptureSessionOutcome::NonRetryableFailure;
                    }
                    Some("authorized") => {
                        let authorized_at =
                            *microphone_authorized_at.get_or_insert_with(tokio::time::Instant::now);
                        if authorized_at.elapsed() >= Duration::from_secs(10) {
                            tracing::warn!(
                                "mic: authorized helper produced no PCM before the startup deadline"
                            );
                            terminate_microphone_helper(&status_path);
                            break CaptureSessionOutcome::NonRetryableFailure;
                        }
                    }
                    // `starting` / `permission_checking`: the macOS consent
                    // prompt is open. Do not count user decision time against
                    // the post-authorization PCM startup watchdog.
                    Some(_) | None => {}
                }
                continue;
            }
            Err(_) => continue, // read timeout — re-check stop
        }
    };
    let _ = std::fs::remove_file(&socket_path);
    outcome
}

fn epoch_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Exponential backoff capped at 5 seconds, mirroring the overlay pattern.
pub fn restart_delay(attempt: u32) -> Duration {
    let base_ms: u64 = 250;
    let capped = attempt.min(6);
    let ms = base_ms.saturating_mul(1u64 << capped);
    Duration::from_millis(ms.min(5_000))
}

/// Whether the continuous system-audio capture task should build an on-device
/// STT provider and emit a live transcript.
///
/// local-first: on-device system-audio STT is ON by default; disable with
/// `BLUEY_SYSTEM_AUDIO_STT=0`. Continuous system-audio streaming is the default
/// capture path (overlay "Listen" + keyless `bluey listen`), so transcription
/// must be on out of the box or "Listen" would capture audio and show nothing.
/// Only an explicit falsey value (`0`/`false`/`off`/`no`) turns it off.
///
/// This module is not behind the `parakeet-stt` feature flag, so the default is
/// unconditional here; the STT factory (`build_stt_chain`) is the layer that
/// actually no-ops the on-device provider when `parakeet-stt` is not compiled
/// in (it returns `NotActive`), so a default-on gate degrades safely.
pub fn is_system_audio_stt_enabled() -> bool {
    !matches!(
        std::env::var("BLUEY_SYSTEM_AUDIO_STT")
            .ok()
            .map(|v| v.trim().to_ascii_lowercase())
            .as_deref(),
        Some("0") | Some("false") | Some("off") | Some("no")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_delay_exponential_and_capped() {
        assert_eq!(restart_delay(0).as_millis(), 250);
        assert_eq!(restart_delay(1).as_millis(), 500);
        assert_eq!(restart_delay(4).as_millis(), 4_000);
        assert_eq!(restart_delay(10).as_millis(), 5_000);
    }

    #[test]
    fn chunk_constants_are_correct() {
        // 20ms at 16kHz = 320 samples
        assert_eq!(CHUNK_SAMPLES, 320);
        assert_eq!(CHUNK_BYTES, 640);
    }

    #[tokio::test]
    async fn start_with_mock_binary() {
        // This test uses the system-audio-stub binary built from this crate
        let stub = std::env::var("BLUEY_SYSTEM_AUDIO_BINARY").ok();
        if stub.is_none() {
            // Try to find the built stub
            let candidate = find_stub_binary();
            if !candidate.exists() {
                // Skip if stub not built
                return;
            }
            std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", &candidate);
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let capture = SystemAudioCapture::start(tx).unwrap();

        // Wait for at least 2 chunks (40ms of audio)
        let mut received = 0;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while received < 2 && tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Some(chunk)) => {
                    assert_eq!(chunk.source, AudioSource::System);
                    assert_eq!(chunk.sample_rate, SampleRate::SR_16K);
                    assert_eq!(chunk.samples.len(), CHUNK_SAMPLES);
                    received += 1;
                }
                _ => break,
            }
        }

        capture.stop().await;
        assert!(received >= 2, "expected at least 2 chunks, got {received}");

        // Clean up env
        if stub.is_none() {
            std::env::remove_var("BLUEY_SYSTEM_AUDIO_BINARY");
        }
    }

    fn find_stub_binary() -> PathBuf {
        let candidates = [
            PathBuf::from("target/debug/system-audio-stub"),
            PathBuf::from("../../target/debug/system-audio-stub"),
            PathBuf::from("../target/debug/system-audio-stub"),
        ];
        for c in candidates {
            if c.exists() {
                return c;
            }
        }
        PathBuf::from("target/debug/system-audio-stub")
    }
}

/// Exit codes from the native system audio helper that indicate permission denial.
/// The macOS ScreenCaptureKit helper exits with code 3 when screen recording
/// permission has not been granted.
const PERMISSION_DENIED_EXIT_CODE: i32 = 3;

/// Check if a child process exit status indicates permission denial.
pub fn is_system_audio_permission_denied(status: std::process::ExitStatus) -> bool {
    status.code() == Some(PERMISSION_DENIED_EXIT_CODE)
}

/// Check if stderr output from the system audio helper indicates permission denial.
pub fn is_system_audio_permission_denied_message(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("permission denied")
        || lower.contains("permission not granted")
        || lower.contains("access denied")
        || lower.contains("access not granted")
        || lower.contains("not authorized")
        || lower.contains("screen recording permission")
        || lower.contains("system audio recording permission")
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[cfg(target_os = "macos")]
    fn microphone_status_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "bluey-microphone-status-test-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn detects_permission_denied_exit_code() {
        assert!(is_system_audio_permission_denied_message(
            "permission denied"
        ));
        assert!(is_system_audio_permission_denied_message(
            "Screen Recording access not granted"
        ));
        assert!(is_system_audio_permission_denied_message(
            "ScreenCaptureKit error: not authorized"
        ));
        assert!(is_system_audio_permission_denied_message(
            "Screen Recording permission required"
        ));
    }

    #[test]
    fn does_not_false_positive_system() {
        assert!(!is_system_audio_permission_denied_message(
            "device not found"
        ));
        assert!(!is_system_audio_permission_denied_message(
            "permission status unavailable"
        ));
        assert!(!is_system_audio_permission_denied_message("timeout"));
        assert!(!is_system_audio_permission_denied_message(""));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn explicit_microphone_denial_is_reported_as_permission_denied() {
        let path = microphone_status_path("denied");
        let _cleanup = TempPathCleanup(path.clone());
        std::fs::write(&path, "permission_denied\n").unwrap();

        assert_eq!(
            microphone_setup_outcome(&path),
            CaptureSessionOutcome::PermissionDenied
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn microphone_launch_failures_are_not_mislabeled_as_permission_denied() {
        let failed_path = microphone_status_path("failed");
        let missing_path = microphone_status_path("missing");
        let _failed_cleanup = TempPathCleanup(failed_path.clone());
        let _missing_cleanup = TempPathCleanup(missing_path.clone());
        std::fs::write(&failed_path, "failed\n").unwrap();
        let _ = std::fs::remove_file(&missing_path);

        assert_eq!(
            microphone_setup_outcome(&failed_path),
            CaptureSessionOutcome::NonRetryableFailure
        );
        assert_eq!(
            microphone_setup_outcome(&missing_path),
            CaptureSessionOutcome::NonRetryableFailure
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn microphone_status_handshake_carries_a_valid_helper_pid() {
        let path = microphone_status_path("pid");
        let _cleanup = TempPathCleanup(path.clone());
        std::fs::write(&path, "authorized\npid=424242\n").unwrap();
        assert_eq!(microphone_helper_pid(&path), Some(424242));

        std::fs::write(&path, format!("authorized\npid={}\n", std::process::id())).unwrap();
        assert_eq!(microphone_helper_pid(&path), None);
    }

    #[tokio::test]
    async fn restart_wait_observes_stop_without_waiting_for_full_backoff() {
        let stop = Arc::new(AtomicBool::new(false));
        let setter = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            setter.store(true, Ordering::Release);
        });

        let started = tokio::time::Instant::now();
        assert!(wait_for_stop(&stop, Duration::from_secs(2)).await);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn permission_exit_is_not_retryable() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(PERMISSION_DENIED_EXIT_CODE << 8);
        assert_eq!(
            classify_helper_exit(Ok(status), ""),
            CaptureSessionOutcome::PermissionDenied
        );
    }

    #[cfg(unix)]
    #[test]
    fn unrelated_helper_failure_remains_retryable() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(1 << 8);
        assert_eq!(
            classify_helper_exit(Ok(status), "device not found"),
            CaptureSessionOutcome::RetryableFailure
        );
    }

    #[test]
    fn terminal_spawn_errors_are_not_retried() {
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidInput,
            std::io::ErrorKind::Unsupported,
        ] {
            let error = std::io::Error::new(kind, "helper launch failed");
            assert_eq!(
                classify_helper_spawn_error(&error),
                CaptureSessionOutcome::NonRetryableFailure
            );
        }
    }

    #[test]
    fn signing_and_profile_spawn_errors_are_not_retried() {
        for message in [
            "code signature invalid",
            "No matching profile found",
            "provisioning profile does not match",
        ] {
            let error = std::io::Error::other(message);
            assert_eq!(
                classify_helper_spawn_error(&error),
                CaptureSessionOutcome::NonRetryableFailure
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn signing_and_dyld_child_exits_are_not_retried() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(1 << 8);
        for message in [
            "No matching profile found",
            "code signature invalid",
            "dyld: Library not loaded: libonnxruntime.dylib",
        ] {
            assert_eq!(
                classify_helper_exit(Ok(status), message),
                CaptureSessionOutcome::NonRetryableFailure
            );
        }
    }

    #[test]
    fn transient_spawn_errors_remain_retryable() {
        let error = std::io::Error::other("temporary launch service interruption");
        assert_eq!(
            classify_helper_spawn_error(&error),
            CaptureSessionOutcome::RetryableFailure
        );
    }
}
