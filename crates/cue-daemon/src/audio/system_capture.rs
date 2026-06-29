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
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

/// Samples per 20 ms chunk at 16 kHz mono.
const CHUNK_SAMPLES: usize = 320;
/// Bytes per chunk: 320 samples * 2 bytes each.
const CHUNK_BYTES: usize = CHUNK_SAMPLES * 2;
/// Maximum consecutive restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Handle to a running system audio capture session.
pub struct SystemAudioCapture {
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
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

        let task = tokio::spawn(async move {
            supervisor_loop(binary, sender, stop_clone, pick).await;
        });

        Ok(Self {
            stop,
            task: Some(task),
        })
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
            for candidate in [dir.join("bluey-audio-macos"), dir.join("cue-audio-macos")] {
                if candidate.exists() {
                    return candidate;
                }
            }
        }
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

async fn spawn_child(binary: &PathBuf, pick: bool) -> std::io::Result<Child> {
    let mut cmd = Command::new(binary);
    if pick {
        // Interactive picker mode: the helper presents the system content-sharing
        // picker, then streams the chosen app's audio (same PCM format).
        cmd.args(["--pick", "--continuous"]);
    } else {
        cmd.args(["--source", "system", "--continuous"]);
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
}

async fn supervisor_loop(
    binary: PathBuf,
    sender: UnboundedSender<AudioChunk>,
    stop: Arc<AtomicBool>,
    pick: bool,
) {
    let mut consecutive_failures: u32 = 0;

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }

        let child = match spawn_child(&binary, pick).await {
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
    sender: &UnboundedSender<AudioChunk>,
    stop: &Arc<AtomicBool>,
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

            if sender.send(chunk).is_err() {
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
