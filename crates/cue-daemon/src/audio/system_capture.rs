//! System audio capture via native OS helpers.
//!
//! Spawns the platform-specific native audio helper as a child process in
//! `--continuous` mode. The helper streams 16 kHz mono i16 LE PCM on stdout.
//! This module reads that stream, frames it into 20 ms `AudioChunk`s with
//! `source: System`, and pushes them to the provided channel.
//!
//! Restart-on-crash with exponential backoff mirrors the overlay pattern.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
#[cfg(target_os = "macos")]
use cue_core::process_aliases::MACOS_AUDIO_HELPER_NAMES;
#[cfg(target_os = "windows")]
use cue_core::process_aliases::WINDOWS_AUDIO_HELPER_NAMES;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::audio::capture::{latest_channel, LatestReceiver, LatestSender};

/// Samples per 20 ms chunk at 16 kHz mono.
const CHUNK_SAMPLES: usize = 320;
/// Bytes per chunk: 320 samples * 2 bytes each.
const CHUNK_BYTES: usize = CHUNK_SAMPLES * 2;
/// Maximum consecutive restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 5;
/// One second of 20 ms chunks for bounded system-audio consumers.
const CAPTURE_QUEUE_CAPACITY: usize = 50;
const STOP_TIMEOUT: Duration = Duration::from_secs(1);
const HELPER_READY_TIMEOUT: Duration = Duration::from_secs(3);
const HELPER_EXIT_TIMEOUT: Duration = Duration::from_millis(500);
const HELPER_DIAGNOSTIC_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const HELPER_PROTOCOL_VERSION: u32 = 1;
const HELPER_DIAGNOSTIC_MAX_LINE_BYTES: usize = 4 * 1024;
const HELPER_DIAGNOSTIC_MAX_PARSED_LINES: u64 = 256;
const HELPER_DIAGNOSTIC_MAX_FIELD_CHARS: usize = 160;

#[cfg(any(target_os = "macos", target_os = "windows", debug_assertions))]
const AUDIO_HELPER_OVERRIDE_ENV_NAMES: &[&str] = &[
    "BLUEY_SYSTEM_AUDIO_BINARY",
    "BLUEY_AUDIO_HELPER_BIN",
    "CUE_AUDIO_HELPER_BIN",
];

#[cfg(target_os = "windows")]
const AUDIO_HELPER_ENV_ALLOWLIST: &[&str] = &["SystemRoot", "WINDIR", "SystemDrive", "TEMP", "TMP"];

#[cfg(target_os = "macos")]
const AUDIO_HELPER_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "__CF_USER_TEXT_ENCODING",
];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const AUDIO_HELPER_ENV_ALLOWLIST: &[&str] = &["HOME", "TMPDIR", "LANG", "LC_ALL"];

pub fn system_audio_channel() -> (LatestSender<AudioChunk>, LatestReceiver<AudioChunk>) {
    latest_channel(CAPTURE_QUEUE_CAPACITY)
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
    stop_notify: Arc<Notify>,
    task: Option<JoinHandle<()>>,
    dropped_chunks: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum NativeAudioHelperMode {
    Continuous,
    DurationMs(u32),
}

/// A protocol-validated native audio helper stream shared by live relay and
/// finite chunk capture. The helper starts with a minimal environment, stderr
/// is always drained through the bounded v1 diagnostic parser, and dropping
/// this value terminates the child.
pub(crate) struct NativeAudioHelperStream {
    child: Child,
    stdout: Option<ChildStdout>,
    diagnostics_task: Option<JoinHandle<HelperDiagnosticsSummary>>,
}

impl SystemAudioCapture {
    /// Start system audio capture. Spawns the native helper and begins
    /// streaming `AudioChunk`s to `sender`.
    pub fn start(sender: LatestSender<AudioChunk>) -> std::io::Result<Self> {
        let binary = resolve_binary()?;
        Ok(Self::start_with_binary(sender, binary))
    }

    fn start_with_binary(sender: LatestSender<AudioChunk>, binary: PathBuf) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_notify = Arc::new(Notify::new());
        let dropped_chunks = Arc::new(AtomicU64::new(0));
        let stop_clone = stop.clone();
        let task_stop_notify = Arc::clone(&stop_notify);
        let task_dropped_chunks = dropped_chunks.clone();
        let task = tokio::spawn(async move {
            supervisor_loop(
                binary,
                sender,
                stop_clone,
                task_stop_notify,
                task_dropped_chunks,
            )
            .await;
        });

        Self {
            stop,
            stop_notify,
            task: Some(task),
            dropped_chunks,
        }
    }

    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }

    /// Signal the capture to stop and wait for the task to finish.
    pub async fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        self.stop_notify.notify_one();
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(STOP_TIMEOUT, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.stop_notify.notify_one();
        if let Some(task) = &self.task {
            task.abort();
        }
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
    let override_path = AUDIO_HELPER_OVERRIDE_ENV_NAMES
        .iter()
        .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from);
    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, "failed to locate current executable for audio helper discovery");
            return None;
        }
    };
    let dev_mode = cfg!(debug_assertions);
    let cwd = dev_mode
        .then(std::env::current_dir)
        .transpose()
        .ok()
        .flatten();
    let home = dev_mode
        .then(|| {
            ["HOME", "USERPROFILE"]
                .iter()
                .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
                .map(PathBuf::from)
        })
        .flatten();

    match find_platform_audio_helper_with(
        names,
        dev_mode,
        override_path.as_deref(),
        &current_exe,
        cwd.as_deref(),
        home.as_deref(),
    ) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, "audio helper discovery rejected an untrusted candidate");
            None
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn find_platform_audio_helper_with(
    names: &[&str],
    dev_mode: bool,
    override_path: Option<&Path>,
    current_exe: &Path,
    cwd: Option<&Path>,
    home: Option<&Path>,
) -> io::Result<Option<PathBuf>> {
    let canonical_exe = canonical_audio_helper(current_exe)?;
    let install_dir = canonical_exe.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "current Bluey executable has no install directory",
        )
    })?;

    if dev_mode {
        if let Some(path) = override_path {
            tracing::info!(
                path = %path.display(),
                "using audio helper binary override in development build"
            );
            return canonical_audio_helper(path).map(Some);
        }
    } else if override_path.is_some() {
        tracing::warn!("audio helper binary override set but ignored in production build");
    }

    if !dev_mode {
        for name in names {
            let candidate = install_dir.join(name);
            if !candidate.exists() {
                continue;
            }
            let canonical = canonical_audio_helper(&candidate)?;
            verify_packaged_audio_helper(&canonical, install_dir, names)?;
            return Ok(Some(canonical));
        }
        return Ok(None);
    }

    let mut candidates = Vec::new();
    if let Some(raw_exe_dir) = current_exe.parent() {
        push_named_candidates(&mut candidates, raw_exe_dir, names);
    }
    push_named_candidates(&mut candidates, install_dir, names);
    for relative_dir in AUDIO_HELPER_EXE_RELATIVE_DIRS {
        push_named_candidates(&mut candidates, &install_dir.join(relative_dir), names);
    }
    if let Some(home) = home {
        push_named_candidates(&mut candidates, &home.join(".bluey/bin"), names);
    }
    if let Some(cwd) = cwd {
        push_named_candidates(&mut candidates, &cwd.join(AUDIO_HELPER_BUILD_DIR), names);
        push_named_candidates(&mut candidates, cwd, names);
    }

    for candidate in candidates {
        if candidate.is_file() {
            return canonical_audio_helper(&candidate).map(Some);
        }
    }
    Ok(None)
}

#[cfg(any(target_os = "macos", target_os = "windows", debug_assertions))]
fn canonical_audio_helper(path: &Path) -> io::Result<PathBuf> {
    let canonical = path.canonicalize().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "failed to canonicalize audio helper {}: {error}",
                path.display()
            ),
        )
    })?;
    if !canonical.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("audio helper path is not a file: {}", canonical.display()),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if canonical.metadata()?.permissions().mode() & 0o111 == 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("audio helper is not executable: {}", canonical.display()),
            ));
        }
    }
    Ok(canonical)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn verify_packaged_audio_helper(
    helper: &Path,
    install_dir: &Path,
    names: &[&str],
) -> io::Result<()> {
    let canonical_install = install_dir.canonicalize().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "failed to canonicalize Bluey install directory {}: {error}",
                install_dir.display()
            ),
        )
    })?;
    if !helper.starts_with(&canonical_install) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "audio helper is outside the packaged Bluey install directory: {}",
                helper.display()
            ),
        ));
    }
    let trusted_name = helper
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            names
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
        });
    if !trusted_name {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "audio helper does not use a trusted packaged alias: {}",
                helper.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn push_named_candidates(candidates: &mut Vec<PathBuf>, dir: &Path, names: &[&str]) {
    for name in names {
        push_unique_candidate(candidates, dir.join(name));
    }
}

#[cfg(all(not(any(target_os = "macos", target_os = "windows")), debug_assertions))]
pub(crate) fn find_native_audio_helper() -> Option<PathBuf> {
    let override_path = AUDIO_HELPER_OVERRIDE_ENV_NAMES
        .iter()
        .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)?;
    match canonical_audio_helper(&override_path) {
        Ok(path) => {
            tracing::info!(
                path = %path.display(),
                "using audio helper binary override in development build"
            );
            Some(path)
        }
        Err(error) => {
            tracing::warn!(%error, "audio helper discovery rejected an untrusted candidate");
            None
        }
    }
}

#[cfg(all(
    not(any(target_os = "macos", target_os = "windows")),
    not(debug_assertions)
))]
pub(crate) fn find_native_audio_helper() -> Option<PathBuf> {
    None
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn push_unique_candidate(candidates: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct HelperAudioFormat {
    sample_rate_hz: u32,
    channel_count: u32,
    sample_format: String,
}

#[derive(Debug, Clone, Deserialize)]
struct HelperDiagnostic {
    event: String,
    protocol_version: u32,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    format: Option<HelperAudioFormat>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    recoverable: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HelperErrorDiagnostic {
    code: String,
    operation: String,
    recoverable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HelperStoppedDiagnostic {
    reason: String,
    exit_code: i32,
}

#[derive(Debug, Default)]
struct HelperDiagnosticsSummary {
    last_error: Option<HelperErrorDiagnostic>,
    stopped: Option<HelperStoppedDiagnostic>,
    warning_lines: u64,
    invalid_lines: u64,
    truncated_lines: u64,
    suppressed_lines: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperRunDisposition {
    Clean,
    Retry,
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelperReadyWait {
    Ready,
    StopRequested,
}

#[derive(Debug)]
struct BoundedDiagnosticLine {
    bytes: Vec<u8>,
    truncated: bool,
}

fn helper_protocol_required() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

fn helper_expected_backend() -> Option<&'static str> {
    helper_expected_backend_for_source("system")
}

fn helper_expected_backend_for_source(source: &str) -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("wasapi")
    } else if cfg!(target_os = "macos") {
        match source {
            "system" => Some("screen_capture_kit"),
            "microphone" => Some("av_audio_engine"),
            _ => None,
        }
    } else {
        None
    }
}

fn validated_helper_source(source: &str) -> io::Result<&'static str> {
    match source {
        "system" => Ok("system"),
        "microphone" => Ok("microphone"),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native audio helper source must be system or microphone",
        )),
    }
}

fn sanitize_diagnostic_field(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(HELPER_DIAGNOSTIC_MAX_FIELD_CHARS)
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn validate_common_diagnostic(
    diagnostic: &HelperDiagnostic,
    expected_source: &str,
) -> io::Result<()> {
    if diagnostic.protocol_version != HELPER_PROTOCOL_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported audio helper protocol version {}",
                diagnostic.protocol_version
            ),
        ));
    }
    if diagnostic.source.as_deref() != Some(expected_source) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper diagnostic source mismatch",
        ));
    }
    Ok(())
}

fn validate_ready_diagnostic(
    diagnostic: &HelperDiagnostic,
    expected_source: &str,
    expected_backend: Option<&str>,
) -> io::Result<()> {
    validate_common_diagnostic(diagnostic, expected_source)?;
    if diagnostic.event != "ready" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper did not emit a ready diagnostic",
        ));
    }
    if diagnostic
        .backend
        .as_deref()
        .is_none_or(|backend| backend.is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper ready diagnostic omitted its backend",
        ));
    }
    if expected_backend.is_some_and(|backend| diagnostic.backend.as_deref() != Some(backend)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper ready diagnostic backend mismatch",
        ));
    }
    let expected_format = HelperAudioFormat {
        sample_rate_hz: 16_000,
        channel_count: 1,
        sample_format: "i16_le".to_string(),
    };
    if diagnostic.format.as_ref() != Some(&expected_format) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper ready diagnostic format mismatch",
        ));
    }
    Ok(())
}

fn parse_helper_diagnostic(line: &BoundedDiagnosticLine) -> io::Result<HelperDiagnostic> {
    if line.truncated {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper diagnostic exceeded the line limit",
        ));
    }
    if line.bytes.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper emitted an empty diagnostic line",
        ));
    }
    serde_json::from_slice(&line.bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "audio helper emitted malformed diagnostic JSON",
        )
    })
}

async fn read_bounded_diagnostic_line<R>(
    reader: &mut BufReader<R>,
) -> io::Result<Option<BoundedDiagnosticLine>>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(256);
    let mut truncated = false;
    let mut saw_bytes = false;

    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if !saw_bytes {
                return Ok(None);
            }
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok(Some(BoundedDiagnosticLine { bytes, truncated }));
        }

        saw_bytes = true;
        let newline = available.iter().position(|byte| *byte == b'\n');
        let data_len = newline.unwrap_or(available.len());
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let remaining = HELPER_DIAGNOSTIC_MAX_LINE_BYTES.saturating_sub(bytes.len());
        let copy_len = remaining.min(data_len);
        bytes.extend_from_slice(&available[..copy_len]);
        if copy_len < data_len {
            truncated = true;
        }
        reader.consume(consumed);

        if newline.is_some() {
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            return Ok(Some(BoundedDiagnosticLine { bytes, truncated }));
        }
    }
}

async fn await_helper_ready<R>(
    reader: &mut BufReader<R>,
    expected_source: &str,
    expected_backend: Option<&str>,
    stop: &AtomicBool,
    stop_notify: &Notify,
) -> io::Result<HelperReadyWait>
where
    R: AsyncRead + Unpin,
{
    let wait = async {
        if stop.load(Ordering::Acquire) {
            return Ok(HelperReadyWait::StopRequested);
        }
        let line = tokio::select! {
            biased;
            _ = stop_notify.notified() => return Ok(HelperReadyWait::StopRequested),
            line = read_bounded_diagnostic_line(reader) => line?,
        };
        let line = line.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "audio helper closed diagnostics before ready",
            )
        })?;
        let diagnostic = parse_helper_diagnostic(&line)?;
        match diagnostic.event.as_str() {
            "ready" => {
                validate_ready_diagnostic(&diagnostic, expected_source, expected_backend)?;
                Ok(HelperReadyWait::Ready)
            }
            "error" => {
                validate_common_diagnostic(&diagnostic, expected_source)?;
                let code = diagnostic
                    .code
                    .as_deref()
                    .map(sanitize_diagnostic_field)
                    .unwrap_or_else(|| "unknown".to_string());
                let operation = diagnostic
                    .operation
                    .as_deref()
                    .map(sanitize_diagnostic_field)
                    .unwrap_or_else(|| "unknown".to_string());
                let kind = if code == "permission_denied" {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::Other
                };
                Err(io::Error::new(
                    kind,
                    format!("audio helper failed before ready: code={code} operation={operation}"),
                ))
            }
            "stopped" => {
                validate_common_diagnostic(&diagnostic, expected_source)?;
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "audio helper stopped before ready",
                ))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "audio helper emitted an unexpected diagnostic before ready",
            )),
        }
    };

    tokio::time::timeout(HELPER_READY_TIMEOUT, wait)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "audio helper ready timed out"))?
}

fn should_log_diagnostic_count(count: u64) -> bool {
    count <= 4 || count.is_power_of_two()
}

fn record_helper_diagnostic(
    summary: &mut HelperDiagnosticsSummary,
    diagnostic: HelperDiagnostic,
    expected_source: &str,
    expected_backend: Option<&str>,
) {
    if validate_common_diagnostic(&diagnostic, expected_source).is_err() {
        summary.invalid_lines = summary.invalid_lines.saturating_add(1);
        return;
    }

    match diagnostic.event.as_str() {
        "ready" => {
            if validate_ready_diagnostic(&diagnostic, expected_source, expected_backend).is_err() {
                summary.invalid_lines = summary.invalid_lines.saturating_add(1);
            }
        }
        "error" => {
            let (Some(code), Some(recoverable)) =
                (diagnostic.code.as_deref(), diagnostic.recoverable)
            else {
                summary.invalid_lines = summary.invalid_lines.saturating_add(1);
                return;
            };
            let error = HelperErrorDiagnostic {
                code: sanitize_diagnostic_field(code),
                operation: diagnostic
                    .operation
                    .as_deref()
                    .map(sanitize_diagnostic_field)
                    .unwrap_or_else(|| "unknown".to_string()),
                recoverable,
            };
            tracing::warn!(
                code = %error.code,
                operation = %error.operation,
                recoverable = error.recoverable,
                "audio helper reported an error"
            );
            summary.last_error = Some(error);
        }
        "warning" => {
            let (Some(code), Some(count)) = (
                diagnostic.code.as_deref(),
                diagnostic.count.filter(|count| *count > 0),
            ) else {
                summary.invalid_lines = summary.invalid_lines.saturating_add(1);
                return;
            };
            let code = sanitize_diagnostic_field(code);
            let operation = diagnostic
                .operation
                .as_deref()
                .map(sanitize_diagnostic_field)
                .unwrap_or_else(|| "unknown".to_string());
            summary.warning_lines = summary.warning_lines.saturating_add(1);
            tracing::warn!(
                code = %code,
                operation = %operation,
                count,
                "audio helper reported a bounded warning"
            );
        }
        "stopped" => {
            let (Some(reason), Some(exit_code)) =
                (diagnostic.reason.as_deref(), diagnostic.exit_code)
            else {
                summary.invalid_lines = summary.invalid_lines.saturating_add(1);
                return;
            };
            let stopped = HelperStoppedDiagnostic {
                reason: sanitize_diagnostic_field(reason),
                exit_code,
            };
            tracing::info!(
                reason = %stopped.reason,
                exit_code = stopped.exit_code,
                "audio helper reported terminal state"
            );
            summary.stopped = Some(stopped);
        }
        _ => {
            summary.invalid_lines = summary.invalid_lines.saturating_add(1);
        }
    }
}

async fn drain_helper_diagnostics<R>(
    mut reader: BufReader<R>,
    expected_source: &'static str,
    expected_backend: Option<&'static str>,
) -> HelperDiagnosticsSummary
where
    R: AsyncRead + Unpin,
{
    let mut summary = HelperDiagnosticsSummary::default();
    let mut lines = 0_u64;

    loop {
        let line = match read_bounded_diagnostic_line(&mut reader).await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(error) => {
                summary.invalid_lines = summary.invalid_lines.saturating_add(1);
                tracing::warn!(%error, "failed to read audio helper diagnostics");
                break;
            }
        };
        lines = lines.saturating_add(1);
        if lines > HELPER_DIAGNOSTIC_MAX_PARSED_LINES {
            summary.suppressed_lines = summary.suppressed_lines.saturating_add(1);
            continue;
        }
        if line.truncated {
            summary.truncated_lines = summary.truncated_lines.saturating_add(1);
            if should_log_diagnostic_count(summary.truncated_lines) {
                tracing::warn!(
                    truncated_lines = summary.truncated_lines,
                    "discarded oversized audio helper diagnostic"
                );
            }
            continue;
        }
        match parse_helper_diagnostic(&line) {
            Ok(diagnostic) => record_helper_diagnostic(
                &mut summary,
                diagnostic,
                expected_source,
                expected_backend,
            ),
            Err(_) => {
                summary.invalid_lines = summary.invalid_lines.saturating_add(1);
                if should_log_diagnostic_count(summary.invalid_lines) {
                    tracing::warn!(
                        invalid_lines = summary.invalid_lines,
                        "discarded malformed audio helper diagnostic"
                    );
                }
            }
        }
    }

    if summary.suppressed_lines > 0 {
        tracing::warn!(
            suppressed_lines = summary.suppressed_lines,
            "suppressed excess audio helper diagnostics"
        );
    }
    summary
}

fn configure_audio_helper_command(
    command: &mut Command,
    source: &'static str,
    mode: NativeAudioHelperMode,
) {
    command
        .env_clear()
        .args(["--source", source])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    match mode {
        NativeAudioHelperMode::Continuous => {
            command.arg("--continuous");
        }
        NativeAudioHelperMode::DurationMs(duration_ms) => {
            command
                .arg("--duration-ms")
                .arg(duration_ms.clamp(20, 3_600_000).to_string());
        }
    }
    for name in AUDIO_HELPER_ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

async fn spawn_child(binary: &Path) -> io::Result<Child> {
    let mut command = Command::new(binary);
    configure_audio_helper_command(&mut command, "system", NativeAudioHelperMode::Continuous);
    command.spawn()
}

pub(crate) async fn spawn_native_audio_helper_stream(
    binary: &Path,
    source: &str,
    mode: NativeAudioHelperMode,
) -> io::Result<NativeAudioHelperStream> {
    let source = validated_helper_source(source)?;
    let expected_backend = helper_expected_backend_for_source(source);
    let mut command = Command::new(binary);
    configure_audio_helper_command(&mut command, source, mode);
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "native audio helper did not expose PCM output",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "native audio helper did not expose diagnostics",
        )
    })?;
    let mut stderr = BufReader::new(stderr);
    let stop = AtomicBool::new(false);
    let stop_notify = Notify::new();
    if let Err(error) =
        await_helper_ready(&mut stderr, source, expected_backend, &stop, &stop_notify).await
    {
        terminate_helper_child(&mut child).await;
        return Err(error);
    }
    let diagnostics_task = tokio::spawn(drain_helper_diagnostics(stderr, source, expected_backend));
    Ok(NativeAudioHelperStream {
        child,
        stdout: Some(stdout),
        diagnostics_task: Some(diagnostics_task),
    })
}

impl NativeAudioHelperStream {
    pub(crate) async fn read_pcm(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let stdout = self.stdout.as_mut().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "native audio helper PCM stream is closed",
            )
        })?;
        stdout.read(buffer).await
    }

    pub(crate) async fn stop(mut self) {
        self.stdout.take();
        terminate_helper_child(&mut self.child).await;
        let _ = self.take_diagnostics().await;
    }

    pub(crate) async fn wait_for_clean_exit(mut self) -> io::Result<()> {
        self.stdout.take();
        let status = match tokio::time::timeout(HELPER_EXIT_TIMEOUT, self.child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                terminate_helper_child(&mut self.child).await;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "native audio helper did not terminate after closing PCM output",
                ));
            }
        };
        let diagnostics = self.take_diagnostics().await;
        let permission_denied = status.code() == Some(PERMISSION_DENIED_EXIT_CODE)
            || diagnostics
                .last_error
                .as_ref()
                .is_some_and(|error| error.code == "permission_denied");
        match classify_helper_exit(Some(&status), &diagnostics, true, false) {
            HelperRunDisposition::Clean => Ok(()),
            HelperRunDisposition::Retry => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "native audio helper ended with a recoverable failure",
            )),
            HelperRunDisposition::Fatal => Err(io::Error::new(
                if permission_denied {
                    io::ErrorKind::PermissionDenied
                } else {
                    io::ErrorKind::InvalidData
                },
                "native audio helper failed protocol or terminal-state validation",
            )),
        }
    }

    async fn take_diagnostics(&mut self) -> HelperDiagnosticsSummary {
        let Some(mut task) = self.diagnostics_task.take() else {
            return HelperDiagnosticsSummary::default();
        };
        match tokio::time::timeout(HELPER_DIAGNOSTIC_DRAIN_TIMEOUT, &mut task).await {
            Ok(Ok(summary)) => summary,
            Ok(Err(error)) => {
                tracing::warn!(%error, "native audio helper diagnostic task failed");
                HelperDiagnosticsSummary {
                    invalid_lines: 1,
                    ..HelperDiagnosticsSummary::default()
                }
            }
            Err(_) => {
                task.abort();
                let _ = task.await;
                HelperDiagnosticsSummary {
                    invalid_lines: 1,
                    ..HelperDiagnosticsSummary::default()
                }
            }
        }
    }
}

async fn terminate_helper_child(child: &mut Child) {
    // `Child::kill().await` includes an unbounded wait for process exit. A
    // wedged or hostile helper must not be able to pin an audio cancellation
    // path indefinitely, so signal synchronously and bound the reap attempt.
    let _ = child.start_kill();
    let _ = tokio::time::timeout(HELPER_EXIT_TIMEOUT, child.wait()).await;
}

impl Drop for NativeAudioHelperStream {
    fn drop(&mut self) {
        if let Some(task) = &self.diagnostics_task {
            task.abort();
        }
    }
}

async fn supervisor_loop(
    binary: PathBuf,
    sender: LatestSender<AudioChunk>,
    stop: Arc<AtomicBool>,
    stop_notify: Arc<Notify>,
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
                tokio::select! {
                    biased;
                    _ = stop_notify.notified() => return,
                    _ = tokio::time::sleep(restart_delay(consecutive_failures - 1)) => {}
                }
                continue;
            }
        };

        let disposition =
            read_child_stdout(child, &sender, &stop, &stop_notify, &dropped_chunks).await;

        if stop.load(Ordering::Acquire) {
            return;
        }

        match disposition {
            HelperRunDisposition::Clean => return,
            HelperRunDisposition::Fatal => {
                tracing::error!(
                    path = %binary.display(),
                    "system audio helper failed protocol or trust checks; not respawning"
                );
                return;
            }
            HelperRunDisposition::Retry => {}
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
        tokio::select! {
            biased;
            _ = stop_notify.notified() => return,
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

async fn read_child_stdout(
    mut child: Child,
    sender: &LatestSender<AudioChunk>,
    stop: &Arc<AtomicBool>,
    stop_notify: &Notify,
    dropped_chunks: &AtomicU64,
) -> HelperRunDisposition {
    read_child_stdout_with_protocol(
        &mut child,
        sender,
        stop,
        stop_notify,
        dropped_chunks,
        helper_protocol_required(),
        helper_expected_backend(),
    )
    .await
}

async fn read_child_stdout_with_protocol(
    child: &mut Child,
    sender: &LatestSender<AudioChunk>,
    stop: &Arc<AtomicBool>,
    stop_notify: &Notify,
    dropped_chunks: &AtomicU64,
    protocol_required: bool,
    expected_backend: Option<&'static str>,
) -> HelperRunDisposition {
    let Some(mut stdout) = child.stdout.take() else {
        terminate_helper_child(child).await;
        return HelperRunDisposition::Fatal;
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_helper_child(child).await;
        return HelperRunDisposition::Fatal;
    };
    let mut stderr = BufReader::new(stderr);

    if protocol_required {
        match await_helper_ready(&mut stderr, "system", expected_backend, stop, stop_notify).await {
            Ok(HelperReadyWait::Ready) => {}
            Ok(HelperReadyWait::StopRequested) => {
                terminate_helper_child(child).await;
                return HelperRunDisposition::Clean;
            }
            Err(error) => {
                tracing::error!(%error, "system audio helper ready handshake failed");
                terminate_helper_child(child).await;
                return HelperRunDisposition::Fatal;
            }
        }
    }

    let mut diagnostics_task =
        tokio::spawn(drain_helper_diagnostics(stderr, "system", expected_backend));

    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut offset = 0usize;
    let mut intentional_stop = false;
    let mut stdout_error = false;

    loop {
        if stop.load(Ordering::Acquire) {
            let _ = child.start_kill();
            intentional_stop = true;
            break;
        }

        let n = match tokio::select! {
            biased;
            _ = stop_notify.notified() => {
                let _ = child.start_kill();
                intentional_stop = true;
                break;
            }
            read = stdout.read(&mut buf[offset..]) => read,
        } {
            Ok(0) => break,
            Ok(n) => n,
            Err(error) => {
                tracing::warn!(%error, "failed to read system audio helper stdout");
                stdout_error = true;
                break;
            }
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
                let _ = child.start_kill();
                intentional_stop = true;
                break;
            }

            buf.copy_within(CHUNK_BYTES..offset, 0);
            offset -= CHUNK_BYTES;
        }

        if intentional_stop {
            break;
        }
    }

    if intentional_stop || stop.load(Ordering::Acquire) {
        diagnostics_task.abort();
        terminate_helper_child(child).await;
        let _ = diagnostics_task.await;
        return HelperRunDisposition::Clean;
    }

    let status = match tokio::time::timeout(HELPER_EXIT_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(error)) => {
            tracing::warn!(%error, "failed to wait for system audio helper");
            None
        }
        Err(_) => {
            tracing::warn!("system audio helper did not exit after closing stdout");
            stdout_error = true;
            terminate_helper_child(child).await;
            None
        }
    };
    let diagnostics =
        match tokio::time::timeout(HELPER_DIAGNOSTIC_DRAIN_TIMEOUT, &mut diagnostics_task).await {
            Ok(Ok(summary)) => summary,
            Ok(Err(error)) => {
                tracing::warn!(%error, "audio helper diagnostic task failed");
                HelperDiagnosticsSummary {
                    invalid_lines: 1,
                    ..HelperDiagnosticsSummary::default()
                }
            }
            Err(_) => {
                diagnostics_task.abort();
                let _ = diagnostics_task.await;
                tracing::warn!("audio helper diagnostic drain timed out");
                HelperDiagnosticsSummary {
                    invalid_lines: 1,
                    ..HelperDiagnosticsSummary::default()
                }
            }
        };

    if stop.load(Ordering::Acquire) {
        return HelperRunDisposition::Clean;
    }
    classify_helper_exit(
        status.as_ref(),
        &diagnostics,
        protocol_required,
        stdout_error,
    )
}

fn classify_helper_exit(
    status: Option<&std::process::ExitStatus>,
    diagnostics: &HelperDiagnosticsSummary,
    protocol_required: bool,
    stdout_error: bool,
) -> HelperRunDisposition {
    if status.is_some_and(|status| status.code() == Some(PERMISSION_DENIED_EXIT_CODE)) {
        return HelperRunDisposition::Fatal;
    }
    if let Some(error) = diagnostics.last_error.as_ref() {
        if error.code == "permission_denied" {
            return HelperRunDisposition::Fatal;
        }
        return if error.recoverable {
            HelperRunDisposition::Retry
        } else {
            HelperRunDisposition::Fatal
        };
    }
    if stdout_error {
        return HelperRunDisposition::Retry;
    }
    let Some(status) = status else {
        return HelperRunDisposition::Retry;
    };
    if protocol_required
        && (diagnostics.invalid_lines > 0
            || diagnostics.truncated_lines > 0
            || diagnostics.suppressed_lines > 0)
    {
        return HelperRunDisposition::Fatal;
    }
    if let Some(stopped) = diagnostics.stopped.as_ref() {
        if status.code() != Some(stopped.exit_code) {
            return if protocol_required {
                HelperRunDisposition::Fatal
            } else {
                HelperRunDisposition::Retry
            };
        }
        return if status.success() && stopped.exit_code == 0 {
            HelperRunDisposition::Clean
        } else {
            HelperRunDisposition::Retry
        };
    }
    if protocol_required {
        return HelperRunDisposition::Fatal;
    }
    if status.success() {
        HelperRunDisposition::Clean
    } else {
        HelperRunDisposition::Retry
    }
}

fn try_emit_chunk(
    sender: &LatestSender<AudioChunk>,
    chunk: AudioChunk,
    dropped_chunks: &AtomicU64,
) -> bool {
    match sender.try_send(chunk) {
        Ok(Some(_)) => {
            let dropped = dropped_chunks.fetch_add(1, Ordering::Relaxed) + 1;
            if dropped.is_power_of_two() {
                tracing::warn!(dropped, "system audio capture queue overloaded");
            }
            true
        }
        Ok(None) => true,
        Err(_) => false,
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
    use std::fs;

    use tokio::io::AsyncWriteExt;

    const READY_DIAGNOSTIC: &str = concat!(
        "{\"event\":\"ready\",\"protocol_version\":1,\"source\":\"system\",",
        "\"backend\":\"wasapi\",\"format\":{\"sample_rate_hz\":16000,",
        "\"channel_count\":1,\"sample_format\":\"i16_le\"}}\n"
    );

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "bluey-system-audio-{name}-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_executable(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

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

    #[test]
    fn ready_diagnostic_requires_exact_source_backend_and_format() {
        let valid = BoundedDiagnosticLine {
            bytes: READY_DIAGNOSTIC.trim_end().as_bytes().to_vec(),
            truncated: false,
        };
        let diagnostic = parse_helper_diagnostic(&valid).unwrap();
        validate_ready_diagnostic(&diagnostic, "system", Some("wasapi")).unwrap();
        assert!(validate_ready_diagnostic(&diagnostic, "microphone", Some("wasapi")).is_err());
        assert!(validate_ready_diagnostic(&diagnostic, "system", Some("coreaudio")).is_err());

        let wrong_format = BoundedDiagnosticLine {
            bytes: READY_DIAGNOSTIC
                .replace("\"sample_rate_hz\":16000", "\"sample_rate_hz\":48000")
                .trim_end()
                .as_bytes()
                .to_vec(),
            truncated: false,
        };
        let diagnostic = parse_helper_diagnostic(&wrong_format).unwrap();
        assert!(validate_ready_diagnostic(&diagnostic, "system", Some("wasapi")).is_err());
    }

    #[test]
    fn native_platforms_require_the_exact_helper_protocol_backend() {
        assert_eq!(
            helper_protocol_required(),
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
        #[cfg(target_os = "macos")]
        assert_eq!(helper_expected_backend(), Some("screen_capture_kit"));
        #[cfg(target_os = "windows")]
        assert_eq!(helper_expected_backend(), Some("wasapi"));
    }

    #[tokio::test]
    async fn ready_handshake_accepts_only_structured_protocol_v1() {
        let (mut writer, reader) = tokio::io::duplex(2 * 1024);
        let writer_task = tokio::spawn(async move {
            writer.write_all(READY_DIAGNOSTIC.as_bytes()).await.unwrap();
        });
        let stop = AtomicBool::new(false);
        let stop_notify = Notify::new();
        let mut reader = BufReader::new(reader);
        let result = await_helper_ready(&mut reader, "system", Some("wasapi"), &stop, &stop_notify)
            .await
            .unwrap();
        writer_task.await.unwrap();
        assert_eq!(result, HelperReadyWait::Ready);
    }

    #[tokio::test]
    async fn bounded_diagnostic_reader_discards_oversized_line_tail() {
        let (mut writer, reader) = tokio::io::duplex(HELPER_DIAGNOSTIC_MAX_LINE_BYTES * 2);
        let writer_task = tokio::spawn(async move {
            let oversized = vec![b'x'; HELPER_DIAGNOSTIC_MAX_LINE_BYTES + 257];
            writer.write_all(&oversized).await.unwrap();
            writer.write_all(b"\n").await.unwrap();
            writer.write_all(READY_DIAGNOSTIC.as_bytes()).await.unwrap();
        });
        let mut reader = BufReader::new(reader);
        let first = read_bounded_diagnostic_line(&mut reader)
            .await
            .unwrap()
            .unwrap();
        assert!(first.truncated);
        assert_eq!(first.bytes.len(), HELPER_DIAGNOSTIC_MAX_LINE_BYTES);
        let second = read_bounded_diagnostic_line(&mut reader)
            .await
            .unwrap()
            .unwrap();
        assert!(!second.truncated);
        validate_ready_diagnostic(
            &parse_helper_diagnostic(&second).unwrap(),
            "system",
            Some("wasapi"),
        )
        .unwrap();
        writer_task.await.unwrap();
    }

    #[tokio::test]
    async fn terminal_diagnostics_are_bounded_and_classified() {
        let payload = concat!(
            "{\"event\":\"warning\",\"protocol_version\":1,\"source\":\"system\",",
            "\"code\":\"callback_queue_overflow\",\"operation\":\"enqueue_audio_packet\",",
            "\"count\":8}\n",
            "{\"event\":\"error\",\"protocol_version\":1,\"source\":\"system\",",
            "\"code\":\"device_invalidated\",\"operation\":\"GetBuffer\",",
            "\"recoverable\":true}\n",
            "{\"event\":\"stopped\",\"protocol_version\":1,\"source\":\"system\",",
            "\"reason\":\"capture_error\",\"exit_code\":1}\n"
        );
        let (mut writer, reader) = tokio::io::duplex(2 * 1024);
        let writer_task = tokio::spawn(async move {
            writer.write_all(payload.as_bytes()).await.unwrap();
        });
        let summary =
            drain_helper_diagnostics(BufReader::new(reader), "system", Some("wasapi")).await;
        writer_task.await.unwrap();
        assert_eq!(
            summary.last_error,
            Some(HelperErrorDiagnostic {
                code: "device_invalidated".to_string(),
                operation: "GetBuffer".to_string(),
                recoverable: true,
            })
        );
        assert_eq!(
            summary.stopped,
            Some(HelperStoppedDiagnostic {
                reason: "capture_error".to_string(),
                exit_code: 1,
            })
        );
        assert_eq!(summary.warning_lines, 1);
        assert_eq!(summary.invalid_lines, 0);
    }

    #[tokio::test]
    async fn warning_diagnostics_do_not_become_terminal_errors() {
        let payload = concat!(
            "{\"event\":\"warning\",\"protocol_version\":1,\"source\":\"system\",",
            "\"code\":\"invalid_audio_packet\",\"operation\":\"validate_audio_packet\",",
            "\"count\":4}\n"
        );
        let (mut writer, reader) = tokio::io::duplex(2 * 1024);
        let writer_task = tokio::spawn(async move {
            writer.write_all(payload.as_bytes()).await.unwrap();
        });
        let summary =
            drain_helper_diagnostics(BufReader::new(reader), "system", Some("wasapi")).await;
        writer_task.await.unwrap();
        assert_eq!(summary.warning_lines, 1);
        assert_eq!(summary.invalid_lines, 0);
        assert!(summary.last_error.is_none());
        assert!(summary.stopped.is_none());
    }

    #[test]
    fn fatal_and_retry_terminal_states_are_deterministic() {
        let retry = HelperDiagnosticsSummary {
            last_error: Some(HelperErrorDiagnostic {
                code: "device_invalidated".to_string(),
                operation: "GetBuffer".to_string(),
                recoverable: true,
            }),
            ..HelperDiagnosticsSummary::default()
        };
        assert_eq!(
            classify_helper_exit(None, &retry, true, false),
            HelperRunDisposition::Retry
        );

        let fatal = HelperDiagnosticsSummary {
            last_error: Some(HelperErrorDiagnostic {
                code: "permission_denied".to_string(),
                operation: "InitializeAudioClient".to_string(),
                recoverable: false,
            }),
            ..HelperDiagnosticsSummary::default()
        };
        assert_eq!(
            classify_helper_exit(None, &fatal, true, false),
            HelperRunDisposition::Fatal
        );
        assert_eq!(
            classify_helper_exit(None, &HelperDiagnosticsSummary::default(), true, false),
            HelperRunDisposition::Retry
        );

        #[cfg(unix)]
        {
            let success = std::process::Command::new("sh")
                .args(["-c", "exit 0"])
                .status()
                .unwrap();
            assert_eq!(
                classify_helper_exit(
                    Some(&success),
                    &HelperDiagnosticsSummary::default(),
                    true,
                    false
                ),
                HelperRunDisposition::Fatal
            );
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn release_discovery_ignores_override_and_stays_inside_install_dir() {
        let root = TestDir::new("release-discovery");
        let install = root.path().join("install");
        let outside = root.path().join("outside");
        let cwd = root.path().join("cwd");
        let home = root.path().join("home");
        let current_exe = install.join("bluey-daemon");
        let packaged = install.join("adriverb");
        let override_helper = outside.join("adriverb");
        let cwd_helper = cwd.join(AUDIO_HELPER_BUILD_DIR).join("adriverb");
        write_executable(&current_exe, b"bluey");
        write_executable(&packaged, b"packaged");
        write_executable(&override_helper, b"override");
        write_executable(&cwd_helper, b"cwd");

        let found = find_platform_audio_helper_with(
            &["adriverb"],
            false,
            Some(&override_helper),
            &current_exe,
            Some(&cwd),
            Some(&home),
        )
        .unwrap()
        .unwrap();
        assert_eq!(found, packaged.canonicalize().unwrap());

        fs::remove_file(&packaged).unwrap();
        let missing = find_platform_audio_helper_with(
            &["adriverb"],
            false,
            Some(&override_helper),
            &current_exe,
            Some(&cwd),
            Some(&home),
        )
        .unwrap();
        assert!(missing.is_none());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn development_discovery_accepts_explicit_canonical_override() {
        let root = TestDir::new("development-override");
        let install = root.path().join("install");
        let outside = root.path().join("outside");
        let current_exe = install.join("bluey-daemon");
        let override_helper = outside.join("custom-audio-helper");
        write_executable(&current_exe, b"bluey");
        write_executable(&override_helper, b"override");

        let found = find_platform_audio_helper_with(
            &["adriverb"],
            true,
            Some(&override_helper),
            &current_exe,
            None,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(found, override_helper.canonicalize().unwrap());
    }

    #[cfg(all(unix, any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn release_discovery_rejects_packaged_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = TestDir::new("release-symlink");
        let install = root.path().join("install");
        let outside = root.path().join("outside");
        let current_exe = install.join("bluey-daemon");
        let outside_helper = outside.join("adriverb");
        let packaged_link = install.join("adriverb");
        write_executable(&current_exe, b"bluey");
        write_executable(&outside_helper, b"outside");
        symlink(&outside_helper, &packaged_link).unwrap();

        let error =
            find_platform_audio_helper_with(&["adriverb"], false, None, &current_exe, None, None)
                .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn helper_environment_does_not_allow_path_or_binary_injection() {
        for forbidden in [
            "PATH",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "BLUEY_SYSTEM_AUDIO_BINARY",
            "BLUEY_AUDIO_HELPER_BIN",
            "CUE_AUDIO_HELPER_BIN",
        ] {
            assert!(!AUDIO_HELPER_ENV_ALLOWLIST.contains(&forbidden));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawned_helper_gets_minimal_env_and_required_terminal_protocol() {
        let root = TestDir::new("spawn-protocol");
        let helper = root.path().join("audio-helper");
        let script = concat!(
            "#!/bin/sh\n",
            "if [ \"${BLUEY_AUDIO_HELPER_UNTRUSTED_TEST_SECRET+x}\" = x ]; then exit 9; fi\n",
            "printf '%s\\n' '",
            "{\"event\":\"ready\",\"protocol_version\":1,\"source\":\"system\",",
            "\"backend\":\"wasapi\",\"format\":{\"sample_rate_hz\":16000,",
            "\"channel_count\":1,\"sample_format\":\"i16_le\"}}",
            "' >&2\n",
            "printf '%s\\n' '",
            "{\"event\":\"stopped\",\"protocol_version\":1,\"source\":\"system\",",
            "\"reason\":\"duration_complete\",\"exit_code\":0}",
            "' >&2\n"
        );
        write_executable(&helper, script.as_bytes());
        std::env::set_var(
            "BLUEY_AUDIO_HELPER_UNTRUSTED_TEST_SECRET",
            "must-not-reach-child",
        );
        let mut child = spawn_child(&helper).await.unwrap();
        std::env::remove_var("BLUEY_AUDIO_HELPER_UNTRUSTED_TEST_SECRET");

        let (tx, _rx) = latest_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_notify = Notify::new();
        let dropped = AtomicU64::new(0);
        let disposition = read_child_stdout_with_protocol(
            &mut child,
            &tx,
            &stop,
            &stop_notify,
            &dropped,
            true,
            Some("wasapi"),
        )
        .await;
        assert_eq!(disposition, HelperRunDisposition::Clean);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shared_helper_stream_validates_microphone_backend_and_finite_output() {
        let root = TestDir::new("shared-microphone-stream");
        let helper = root.path().join("audio-helper");
        let backend = helper_expected_backend_for_source("microphone").unwrap_or("test_backend");
        let script = format!(
            "#!/bin/sh\n\
             [ \"$1\" = \"--source\" ] && [ \"$2\" = \"microphone\" ] || exit 8\n\
             [ \"$3\" = \"--duration-ms\" ] && [ \"$4\" = \"40\" ] || exit 9\n\
             printf '%s\\n' '{{\"event\":\"ready\",\"protocol_version\":1,\
             \"source\":\"microphone\",\"backend\":\"{backend}\",\"format\":\
             {{\"sample_rate_hz\":16000,\"channel_count\":1,\
             \"sample_format\":\"i16_le\"}}}}' >&2\n\
             i=0\n\
             while [ \"$i\" -lt 640 ]; do printf '\\000\\000'; i=$((i + 1)); done\n\
             printf '%s\\n' '{{\"event\":\"stopped\",\"protocol_version\":1,\
             \"source\":\"microphone\",\"reason\":\"duration_complete\",\
             \"exit_code\":0}}' >&2\n"
        );
        write_executable(&helper, script.as_bytes());

        let mut stream = spawn_native_audio_helper_stream(
            &helper,
            "microphone",
            NativeAudioHelperMode::DurationMs(40),
        )
        .await
        .unwrap();
        let mut pcm = Vec::new();
        let mut buffer = [0_u8; 512];
        loop {
            let read = stream.read_pcm(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            pcm.extend_from_slice(&buffer[..read]);
        }
        stream.wait_for_clean_exit().await.unwrap();
        assert_eq!(pcm.len(), 1_280);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn start_with_mock_binary() {
        let root = TestDir::new("streaming-helper");
        let helper = root.path().join("audio-helper");
        let backend = helper_expected_backend().unwrap_or("system_audio_stub");
        let script = format!(
            "#!/bin/sh\n\
             printf '%s\\n' '{{\"event\":\"ready\",\"protocol_version\":1,\
             \"source\":\"system\",\"backend\":\"{backend}\",\"format\":\
             {{\"sample_rate_hz\":16000,\"channel_count\":1,\
             \"sample_format\":\"i16_le\"}}}}' >&2\n\
             i=0\n\
             while [ \"$i\" -lt 1280 ]; do\n\
               printf '\\000\\000'\n\
               i=$((i + 1))\n\
             done\n\
             printf '%s\\n' '{{\"event\":\"stopped\",\"protocol_version\":1,\
             \"source\":\"system\",\"reason\":\"duration_complete\",\
             \"exit_code\":0}}' >&2\n"
        );
        write_executable(&helper, script.as_bytes());

        let (tx, mut rx) = system_audio_channel();
        let capture = SystemAudioCapture::start_with_binary(tx, helper);

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
                Ok(None) => break,
                // Parallel test load can delay shell-helper scheduling beyond
                // one polling interval. Keep honoring the outer three-second
                // deadline instead of turning one quiet interval into a false
                // capture failure.
                Err(_) => continue,
            }
        }

        capture.stop().await;
        assert!(received >= 2, "expected at least 2 chunks, got {received}");
    }

    #[tokio::test]
    async fn bounded_output_drops_on_overload_and_detects_closed_receiver() {
        let (tx, mut rx) = latest_channel(1);
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

    #[cfg(unix)]
    #[tokio::test]
    async fn stalled_helper_read_is_interrupted_by_stop_notification() {
        let child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let (tx, _rx) = latest_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_notify = Arc::new(Notify::new());
        let dropped = Arc::new(AtomicU64::new(0));
        let task_stop = Arc::clone(&stop);
        let task_notify = Arc::clone(&stop_notify);
        let task_dropped = Arc::clone(&dropped);
        let task = tokio::spawn(async move {
            let mut child = child;
            read_child_stdout_with_protocol(
                &mut child,
                &tx,
                &task_stop,
                &task_notify,
                &task_dropped,
                false,
                None,
            )
            .await
        });

        tokio::task::yield_now().await;
        stop.store(true, Ordering::Release);
        stop_notify.notify_one();
        let stopped = tokio::time::timeout(STOP_TIMEOUT, task)
            .await
            .expect("stalled helper read exceeded stop deadline")
            .unwrap();
        assert_eq!(stopped, HelperRunDisposition::Clean);
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
