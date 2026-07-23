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
use tokio::process::Command;
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

/// Which capture the native helper runs — selects the helper's `--source` arg
/// and the `AudioChunk.source` tag. Microphone mode enables Apple's
/// VoiceProcessingIO acoustic echo cancellation in the helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureRole {
    System,
    Microphone,
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

        let task = tokio::spawn(async move {
            supervisor_loop(binary, sender, stop_clone, pick, CaptureRole::System).await;
        });

        Ok(Self {
            stop,
            task: Some(task),
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

        let task = tokio::spawn(async move {
            supervisor_loop(binary, sender, stop_clone, false, CaptureRole::Microphone).await;
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
        let task = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            let mut base_ms: u64 = 0;
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
        });
        Ok(Self {
            stop,
            task: Some(task),
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
) {
    let mut consecutive_failures: u32 = 0;

    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }

        // The MIC helper MUST be launched as the .app bundle (macOS reads
        // NSMicrophoneUsageDescription from the bundle Info.plist — a bare exec
        // lacks it and macOS traps on mic access), so it uses the open+socket
        // transport. SYSTEM audio works fine as a bare exec over stdout.
        #[cfg(target_os = "macos")]
        let clean = if role == CaptureRole::Microphone {
            if let Some(bundle) = macos_app_bundle_path() {
                run_mic_socket_session(&bundle, &sender, &stop).await
            } else {
                // No bundle (dev build with only a bare binary) — fall back to
                // stdout. The mic will trap without the plist, but this keeps a
                // plain checkout from failing to spawn; system audio still works.
                run_stdout_session(&binary, pick, role, &sender, &stop).await
            }
        } else {
            run_stdout_session(&binary, pick, role, &sender, &stop).await
        };
        #[cfg(not(target_os = "macos"))]
        let clean = run_stdout_session(&binary, pick, role, &sender, &stop).await;

        if stop.load(Ordering::Acquire) || clean {
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

/// Bare-exec + stdout transport (system audio; non-macOS; macOS dev fallback).
async fn run_stdout_session(
    binary: &std::path::Path,
    pick: bool,
    role: CaptureRole,
    sender: &UnboundedSender<AudioChunk>,
    stop: &Arc<AtomicBool>,
) -> bool {
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
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "failed to spawn native audio helper");
            return false;
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        return false;
    };
    let mut buf = vec![0u8; CHUNK_BYTES.max(8192)];
    let mut offset = 0usize;
    loop {
        if stop.load(Ordering::Acquire) {
            let _ = child.kill().await;
            return true;
        }
        match stdout.read(&mut buf[offset..]).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                offset += n;
                if !drain_frames(&mut buf, &mut offset, sender, role) {
                    let _ = child.kill().await;
                    return true;
                }
            }
            Err(_) => break,
        }
    }
    let status = child.wait().await;
    matches!(status, Ok(s) if s.success())
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
) -> bool {
    use tokio::net::UnixListener;

    let socket_path = std::env::temp_dir().join(format!("bluey-mic-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(error = %e, "mic: failed to bind capture socket");
            return false;
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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    if !matches!(cmd.status().await, Ok(s) if s.success()) {
        tracing::warn!("mic: `open` failed to launch BlueyAudio.app");
        let _ = std::fs::remove_file(&socket_path);
        return false;
    }

    let accept = tokio::time::timeout(Duration::from_secs(15), listener.accept()).await;
    let (mut conn, _addr) = match accept {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "mic: socket accept failed");
            let _ = std::fs::remove_file(&socket_path);
            return false;
        }
        Err(_) => {
            tracing::warn!("mic: helper did not connect before timeout");
            let _ = std::fs::remove_file(&socket_path);
            return false;
        }
    };

    let mut buf = vec![0u8; CHUNK_BYTES.max(8192)];
    let mut offset = 0usize;
    let clean = loop {
        if stop.load(Ordering::Acquire) {
            break true;
        }
        // Short read timeout so the stop flag is checked promptly even when the
        // mic is silent.
        let read =
            tokio::time::timeout(Duration::from_millis(250), conn.read(&mut buf[offset..])).await;
        match read {
            Ok(Ok(0)) => break false, // EOF: helper exited
            Ok(Ok(n)) => {
                offset += n;
                if !drain_frames(&mut buf, &mut offset, sender, CaptureRole::Microphone) {
                    break true; // receiver dropped
                }
            }
            Ok(Err(_)) => break false,
            Err(_) => continue, // read timeout — re-check stop
        }
    };
    let _ = std::fs::remove_file(&socket_path);
    clean
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
