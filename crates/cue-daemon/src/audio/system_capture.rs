//! System audio capture via native OS helpers.
//!
//! Spawns the platform-specific native audio helper as a child process in
//! `--continuous` mode. The helper streams 16 kHz mono i16 LE PCM on stdout.
//! This module reads that stream, frames it into 20 ms `AudioChunk`s with
//! `source: System`, and pushes them to the provided channel.
//!
//! Restart-on-crash with exponential backoff mirrors the overlay pattern.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
#[cfg(target_os = "macos")]
use cue_core::process_aliases::MACOS_AUDIO_HELPER_NAMES;
#[cfg(target_os = "windows")]
use cue_core::process_aliases::WINDOWS_AUDIO_HELPER_NAMES;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

/// Samples per 20 ms chunk at 16 kHz mono.
const CHUNK_SAMPLES: usize = 320;
/// Bytes per chunk: 320 samples * 2 bytes each.
const CHUNK_BYTES: usize = CHUNK_SAMPLES * 2;
/// Maximum consecutive restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 5;
/// One second of 20 ms chunks for bounded system-audio consumers.
const CAPTURE_QUEUE_CAPACITY: usize = 50;

pub fn system_audio_channel() -> (Sender<AudioChunk>, Receiver<AudioChunk>) {
    tokio::sync::mpsc::channel(CAPTURE_QUEUE_CAPACITY)
}

#[cfg(target_os = "macos")]
const AUDIO_HELPER_BUILD_DIR: &str = "native/macos/cue-audio/.build";

#[cfg(target_os = "macos")]
const AUDIO_HELPER_EXE_RELATIVE_DIRS: &[&str] = &[
    "../../native/macos/cue-audio/.build",
    "../native/macos/cue-audio/.build",
];

#[cfg(target_os = "windows")]
const AUDIO_HELPER_BUILD_DIR: &str = "native/windows/cue-audio/build";

#[cfg(target_os = "windows")]
const AUDIO_HELPER_EXE_RELATIVE_DIRS: &[&str] = &[
    "../../native/windows/cue-audio/build",
    "../native/windows/cue-audio/build",
];

/// Handle to a running system audio capture session.
pub struct SystemAudioCapture {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
    dropped_chunks: Arc<AtomicU64>,
}

impl SystemAudioCapture {
    /// Start system audio capture. Spawns the native helper and begins
    /// streaming `AudioChunk`s to `sender`.
    pub fn start(sender: Sender<AudioChunk>) -> std::io::Result<Self> {
        let binary = resolve_binary()?;
        let stop = Arc::new(AtomicBool::new(false));
        let dropped_chunks = Arc::new(AtomicU64::new(0));
        let stop_clone = stop.clone();
        let task_dropped_chunks = dropped_chunks.clone();
        let task = tokio::spawn(async move {
            supervisor_loop(binary, sender, stop_clone, task_dropped_chunks).await;
        });

        Ok(Self {
            stop,
            task: Some(task),
            dropped_chunks,
        })
    }

    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }

    /// Signal the capture to stop and wait for the task to finish.
    pub async fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
        }
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn resolve_binary() -> std::io::Result<PathBuf> {
    find_native_audio_helper().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Bluey system audio helper was not found",
        )
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn find_native_audio_helper() -> Option<PathBuf> {
    find_platform_audio_helper(MACOS_AUDIO_HELPER_NAMES)
}

#[cfg(target_os = "windows")]
pub(crate) fn find_native_audio_helper() -> Option<PathBuf> {
    find_platform_audio_helper(WINDOWS_AUDIO_HELPER_NAMES)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn find_platform_audio_helper(names: &[&str]) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    for env_name in [
        "BLUEY_SYSTEM_AUDIO_BINARY",
        "BLUEY_AUDIO_HELPER_BIN",
        "CUE_AUDIO_HELPER_BIN",
    ] {
        if let Some(path) = std::env::var_os(env_name).filter(|value| !value.is_empty()) {
            push_unique_candidate(&mut candidates, PathBuf::from(path));
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut executable_dirs = Vec::new();
        if let Some(parent) = exe.parent() {
            push_unique_candidate(&mut executable_dirs, parent.to_path_buf());
        }
        if let Ok(canonical) = exe.canonicalize() {
            if let Some(parent) = canonical.parent() {
                push_unique_candidate(&mut executable_dirs, parent.to_path_buf());
            }
        }
        for dir in executable_dirs {
            push_named_candidates(&mut candidates, &dir, names);
            for relative_dir in AUDIO_HELPER_EXE_RELATIVE_DIRS {
                push_named_candidates(&mut candidates, &dir.join(relative_dir), names);
            }
        }
    }

    for home_var in ["HOME", "USERPROFILE"] {
        if let Some(home) = std::env::var_os(home_var) {
            push_named_candidates(
                &mut candidates,
                &PathBuf::from(home).join(".bluey/bin"),
                names,
            );
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        push_named_candidates(&mut candidates, &cwd.join(AUDIO_HELPER_BUILD_DIR), names);
        push_named_candidates(&mut candidates, &cwd, names);
    }

    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn push_named_candidates(candidates: &mut Vec<PathBuf>, dir: &Path, names: &[&str]) {
    for name in names {
        push_unique_candidate(candidates, dir.join(name));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn find_native_audio_helper() -> Option<PathBuf> {
    None
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn push_unique_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

async fn spawn_child(binary: &PathBuf) -> std::io::Result<Child> {
    Command::new(binary)
        .args(["--source", "system", "--continuous"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
}

async fn supervisor_loop(
    binary: PathBuf,
    sender: Sender<AudioChunk>,
    stop: Arc<AtomicBool>,
    dropped_chunks: Arc<AtomicU64>,
) {
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

        let exited_cleanly = read_child_stdout(child, &sender, &stop, &dropped_chunks).await;

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
    dropped_chunks: &AtomicU64,
) -> bool {
    let Some(mut stdout) = child.stdout.take() else {
        return false;
    };

    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut offset = 0usize;

    loop {
        if stop.load(Ordering::Acquire) {
            let _ = child.kill().await;
            return true;
        }

        let n = match stdout.read(&mut buf[offset..]).await {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(_) => break,
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

            if !try_emit_chunk(sender, chunk, dropped_chunks) {
                // Receiver dropped
                let _ = child.kill().await;
                return true;
            }

            buf.copy_within(CHUNK_BYTES..offset, 0);
            offset -= CHUNK_BYTES;
        }
    }

    let status = child.wait().await;
    matches!(status, Ok(s) if s.success())
}

fn try_emit_chunk(
    sender: &Sender<AudioChunk>,
    chunk: AudioChunk,
    dropped_chunks: &AtomicU64,
) -> bool {
    match sender.try_send(chunk) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            let dropped = dropped_chunks.fetch_add(1, Ordering::Relaxed) + 1;
            if dropped.is_power_of_two() {
                tracing::warn!(dropped, "system audio capture queue overloaded");
            }
            true
        }
        Err(TrySendError::Closed(_)) => false,
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

        let (tx, mut rx) = system_audio_channel();
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

    #[tokio::test]
    async fn bounded_output_drops_on_overload_and_detects_closed_receiver() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let dropped = AtomicU64::new(0);
        let chunk = AudioChunk {
            source: AudioSource::System,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0; CHUNK_SAMPLES],
            captured_at_ms: 0,
        };

        assert!(try_emit_chunk(&tx, chunk.clone(), &dropped));
        assert!(try_emit_chunk(&tx, chunk.clone(), &dropped));
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(rx.recv().await.unwrap().samples.len(), chunk.samples.len());
        drop(rx);
        assert!(!try_emit_chunk(&tx, chunk, &dropped));
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
