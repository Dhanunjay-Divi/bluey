//! System audio capture via native OS helpers.
//!
//! Spawns the platform-specific native audio helper as a child process in
//! `--continuous` mode. The helper streams 16 kHz mono i16 LE PCM on stdout.
//! This module reads that stream, frames it into 20 ms `AudioChunk`s with
//! `source: System`, and pushes them to the provided channel.
//!
//! Restart-on-crash with exponential backoff mirrors the overlay pattern.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

use super::helper_diagnostics::{HelperDiagnosticEvent, HelperStderrDiagnostics};

/// Samples per 20 ms chunk at 16 kHz mono.
const CHUNK_SAMPLES: usize = 320;
/// Bytes per chunk: 320 samples * 2 bytes each.
const CHUNK_BYTES: usize = CHUNK_SAMPLES * 2;
/// 200 ms maximum queued audio. Slow consumers cause bounded drops instead of
/// allowing helper output to grow without limit.
pub(crate) const SYSTEM_AUDIO_CHANNEL_CAPACITY: usize = 10;
const CHANNEL_BACKPRESSURE_LIMIT: Duration = Duration::from_millis(40);
const STDERR_JOIN_TIMEOUT: Duration = Duration::from_millis(250);
const STDOUT_READ_POLL: Duration = Duration::from_millis(100);
const HELPER_READINESS_DEADLINE: Duration = Duration::from_secs(3);
/// Maximum consecutive restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Handle to a running system audio capture session.
pub struct SystemAudioCapture {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}

impl SystemAudioCapture {
    /// Start system audio capture. Spawns the native helper and begins
    /// streaming `AudioChunk`s to `sender`.
    pub fn start(sender: Sender<AudioChunk>) -> std::io::Result<Self> {
        let binary = resolve_binary()?;
        Ok(Self::start_with_binary(binary, sender))
    }

    fn start_with_binary(binary: PathBuf, sender: Sender<AudioChunk>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let task = tokio::spawn(async move {
            supervisor_loop(binary, sender, stop_clone).await;
        });

        Self {
            stop,
            task: Some(task),
        }
    }

    /// Signal the capture to stop and wait for the task to finish.
    pub async fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(Duration::from_secs(3), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = tokio::time::timeout(Duration::from_millis(250), task).await;
            }
        }
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn resolve_binary() -> std::io::Result<PathBuf> {
    // Allow override for testing
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("BLUEY_SYSTEM_AUDIO_BINARY") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    let path = platform_binary_path();
    if path.exists() {
        #[cfg(windows)]
        if !super::helper_trust::packaged_windows_helper_integrity_matches(&path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Windows audio helper failed Bluey package integrity verification",
            ));
        }
        return Ok(path);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("system audio binary not found at {}", path.display()),
    ))
}

#[cfg(target_os = "macos")]
fn platform_binary_path() -> PathBuf {
    // Look relative to the daemon binary first
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
                dir.join("audio-driver"),
                dir.join("bluey-audio-macos"),
                dir.join("cue-audio-macos"),
            ] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    for candidate in [
        PathBuf::from("native/macos/cue-audio/.build/audio-driver"),
        PathBuf::from("native/macos/cue-audio/.build/bluey-audio-macos"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("native/macos/cue-audio/.build/audio-driver")
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
            for candidate in [
                dir.join("audio-driver.exe"),
                dir.join("bluey-audio.exe"),
                dir.join("cue-audio.exe"),
            ] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    for candidate in [
        PathBuf::from("native/windows/cue-audio/build/audio-driver.exe"),
        PathBuf::from("native/windows/cue-audio/build/bluey-audio.exe"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("native/windows/cue-audio/build/audio-driver.exe")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_binary_path() -> PathBuf {
    PathBuf::from("bluey-audio")
}

async fn spawn_child(binary: &PathBuf) -> std::io::Result<Child> {
    let mut command = Command::new(binary);
    command
        .args(["--source", "system", "--continuous"])
        .env_clear()
        .envs(native_helper_environment())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    command.spawn()
}

fn native_helper_environment() -> Vec<(String, String)> {
    #[cfg(windows)]
    const ALLOWLIST: &[&str] = &["SystemRoot", "WINDIR", "TEMP", "TMP"];
    #[cfg(target_os = "macos")]
    const ALLOWLIST: &[&str] = &["TMPDIR"];
    #[cfg(not(any(windows, target_os = "macos")))]
    const ALLOWLIST: &[&str] = &[];

    ALLOWLIST
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_string(), value))
        })
        .collect()
}

async fn supervisor_loop(binary: PathBuf, sender: Sender<AudioChunk>, stop: Arc<AtomicBool>) {
    let mut consecutive_failures: u32 = 0;

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }

        let child = match spawn_child(&binary).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to spawn system audio helper");
                consecutive_failures += 1;
                if consecutive_failures > MAX_RESTART_ATTEMPTS {
                    tracing::error!("system audio helper failed too many times; giving up");
                    return;
                }
                tokio::time::sleep(restart_delay(consecutive_failures - 1)).await;
                continue;
            }
        };

        let exited_cleanly = read_child_stdout(child, &sender, &stop).await;

        if stop.load(Ordering::Acquire) {
            return;
        }

        if exited_cleanly {
            // Clean exit means intentional stop
            return;
        }

        consecutive_failures += 1;
        if consecutive_failures > MAX_RESTART_ATTEMPTS {
            tracing::error!(
                attempts = consecutive_failures,
                "system audio helper failed too many times; giving up"
            );
            return;
        }

        let delay = restart_delay(consecutive_failures - 1);
        tracing::warn!(
            attempt = consecutive_failures,
            delay_ms = delay.as_millis() as u64,
            "system audio helper exited unexpectedly; respawning"
        );
        tokio::time::sleep(delay).await;
    }
}

async fn read_child_stdout(
    mut child: Child,
    sender: &Sender<AudioChunk>,
    stop: &Arc<AtomicBool>,
) -> bool {
    let Some(mut stdout) = child.stdout.take() else {
        return false;
    };
    let helper_ready = Arc::new(AtomicBool::new(false));
    let mut stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(drain_bounded_helper_stderr(
            stderr,
            Arc::clone(&helper_ready),
        ))
    });

    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut offset = 0usize;
    let mut dropped_chunks = 0_u64;
    let readiness_started = tokio::time::Instant::now();

    loop {
        if stop.load(Ordering::Acquire) {
            let _ = child.kill().await;
            finish_stderr_task(&mut stderr_task).await;
            return true;
        }

        if !helper_ready.load(Ordering::Acquire)
            && readiness_started.elapsed() >= HELPER_READINESS_DEADLINE
        {
            tracing::warn!("system audio helper readiness deadline elapsed");
            let _ = child.kill().await;
            finish_stderr_task(&mut stderr_task).await;
            return false;
        }

        let n = match tokio::time::timeout(STDOUT_READ_POLL, stdout.read(&mut buf[offset..])).await
        {
            Err(_) => continue,
            Ok(Ok(0)) => break, // EOF
            Ok(Ok(n)) => n,
            Ok(Err(_)) => break,
        };

        offset += n;

        while offset >= CHUNK_BYTES {
            let samples: Vec<i16> = buf[..CHUNK_BYTES]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();

            let chunk = AudioChunk {
                source: AudioSource::System,
                sample_rate: SampleRate::SR_16K,
                samples,
                captured_at_ms: epoch_ms(),
            };

            if helper_ready.load(Ordering::Acquire) {
                match tokio::time::timeout(CHANNEL_BACKPRESSURE_LIMIT, sender.send(chunk)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => {
                        // Receiver dropped.
                        let _ = child.kill().await;
                        finish_stderr_task(&mut stderr_task).await;
                        return true;
                    }
                    Err(_) => {
                        dropped_chunks = dropped_chunks.saturating_add(1);
                        if dropped_chunks == 1 || dropped_chunks.is_multiple_of(100) {
                            tracing::warn!(
                                dropped_chunks,
                                "system audio queue remained full; dropped a bounded chunk"
                            );
                        }
                    }
                }
            }

            buf.copy_within(CHUNK_BYTES..offset, 0);
            offset -= CHUNK_BYTES;
        }
    }

    let status = child.wait().await;
    finish_stderr_task(&mut stderr_task).await;
    matches!(status, Ok(s) if s.success())
}

async fn drain_bounded_helper_stderr(
    mut stderr: tokio::process::ChildStderr,
    helper_ready: Arc<AtomicBool>,
) {
    let mut diagnostics = HelperStderrDiagnostics::default();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = match stderr.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                tracing::debug!(%error, "system audio helper stderr read failed");
                break;
            }
        };
        for event in diagnostics.push(&buffer[..read]) {
            apply_system_audio_helper_event(&helper_ready, event);
        }
    }
    for event in diagnostics.finish() {
        apply_system_audio_helper_event(&helper_ready, event);
    }
    tracing::debug!(
        dropped_lines = diagnostics.dropped_tail_lines(),
        oversized_lines = diagnostics.oversized_lines(),
        malformed_lines = diagnostics.malformed_structured_lines(),
        "system audio helper diagnostics finished"
    );
}

fn apply_system_audio_helper_event(helper_ready: &AtomicBool, event: HelperDiagnosticEvent) {
    match event {
        HelperDiagnosticEvent::Ready { source, format, .. }
            if source == Some(cue_core::AudioSourceKind::System)
                && format == Some(cue_core::AudioStreamFormat::native_helper_pcm16_mono()) =>
        {
            helper_ready.store(true, Ordering::Release);
        }
        HelperDiagnosticEvent::PermissionDenied { source, .. } => {
            tracing::warn!(source = ?source, "system audio helper reported permission denied");
        }
        HelperDiagnosticEvent::Error {
            source,
            recoverable,
            ..
        } => {
            tracing::warn!(source = ?source, recoverable, "system audio helper reported an error");
        }
        HelperDiagnosticEvent::Stopped { source, .. } => {
            tracing::debug!(source = ?source, "system audio helper stopped");
        }
        HelperDiagnosticEvent::Ready { source, .. } => {
            tracing::warn!(source = ?source, "system audio helper readiness contract mismatch");
        }
    }
}

async fn finish_stderr_task(task: &mut Option<JoinHandle<()>>) {
    if let Some(mut task) = task.take() {
        if tokio::time::timeout(STDERR_JOIN_TIMEOUT, &mut task)
            .await
            .is_err()
        {
            task.abort();
            tracing::debug!("system audio helper stderr task did not finish within its bound");
        }
    }
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

/// Check if system audio STT is enabled via env var.
pub fn is_system_audio_stt_enabled() -> bool {
    std::env::var("BLUEY_SYSTEM_AUDIO_STT")
        .map(|v| v == "1")
        .unwrap_or(false)
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
        assert_eq!(SYSTEM_AUDIO_CHANNEL_CAPACITY * 20, 200);
    }

    #[test]
    fn helper_environment_is_explicit_and_never_contains_secrets() {
        let names = native_helper_environment()
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        assert!(names.iter().all(|name| matches!(
            name.as_str(),
            "SystemRoot" | "WINDIR" | "TEMP" | "TMP" | "TMPDIR"
        )));
        assert!(names.iter().all(|name| {
            let upper = name.to_ascii_uppercase();
            !upper.contains("TOKEN") && !upper.contains("KEY") && !upper.starts_with("BLUEY_")
        }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn silent_helper_stop_is_bounded_and_aborts_the_child() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!(
            "bluey-silent-audio-helper-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let helper = base.join("silent-helper.sh");
        std::fs::write(&helper, b"#!/bin/sh\nsleep 30\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let capture = SystemAudioCapture::start_with_binary(helper, tx);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let started = tokio::time::Instant::now();
        capture.stop().await;
        assert!(started.elapsed() < Duration::from_secs(1));
        let _ = std::fs::remove_dir_all(base);
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

        let (tx, mut rx) = tokio::sync::mpsc::channel(SYSTEM_AUDIO_CHANNEL_CAPACITY);
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
    lower.contains("permission")
        || lower.contains("screen recording")
        || lower.contains("screencapturekit")
        || lower.contains("not authorized")
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn detects_permission_denied_exit_code() {
        // We can't easily construct ExitStatus with a specific code in tests
        // on all platforms, so test the message classifier instead.
        assert!(is_system_audio_permission_denied_message(
            "permission denied"
        ));
        assert!(is_system_audio_permission_denied_message(
            "Screen Recording access not granted"
        ));
        assert!(is_system_audio_permission_denied_message(
            "ScreenCaptureKit error: not authorized"
        ));
    }

    #[test]
    fn does_not_false_positive_system() {
        assert!(!is_system_audio_permission_denied_message(
            "device not found"
        ));
        assert!(!is_system_audio_permission_denied_message("timeout"));
        assert!(!is_system_audio_permission_denied_message(""));
    }
}
