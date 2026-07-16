//! Trusted launcher contract for Bluey's native Windows screen-capture helper.
//!
//! Authorization remains with the caller: this module only discovers and runs
//! the packaged helper after an explicit CLI request or approved Context-mode
//! fallback. It never initiates capture on its own.

use std::env;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::process_aliases::WINDOWS_CAPTURE_HELPER_NAMES;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const CAPTURE_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_DIAGNOSTIC_BYTES: usize = 32 * 1024;
const MAX_CAPTURE_PIXELS: u64 = 100_000_000;
const CAPTURE_BYTES_PER_PIXEL: u64 = 4;
const MAX_CAPTURE_PIXEL_BYTES: u64 = MAX_CAPTURE_PIXELS * CAPTURE_BYTES_PER_PIXEL;
const MAX_CAPTURE_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
// The helper receives no credentials, provider keys, browser variables, PATH,
// or application configuration. These four values are retained only for
// Windows runtime/temp-directory compatibility.
const WINDOWS_CAPTURE_ENV_ALLOWLIST: &[&str] = &["SystemRoot", "WINDIR", "TEMP", "TMP"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsCaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WindowsCaptureRegion {
    pub fn from_values(values: [i64; 4]) -> Result<Self> {
        let [x, y, width, height] = values;
        let x = i32::try_from(x).context("capture region x is outside the Windows range")?;
        let y = i32::try_from(y).context("capture region y is outside the Windows range")?;
        let width =
            i32::try_from(width).context("capture region width is outside the Windows range")?;
        let height =
            i32::try_from(height).context("capture region height is outside the Windows range")?;
        checked_capture_pixel_bytes(width, height)?;
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsCaptureDiagnostic {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub pixel_bytes: u64,
    pub output_bytes: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Deserialize)]
struct CaptureHelperEvent {
    event: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    recoverable: Option<bool>,
    #[serde(default)]
    win32_error: Option<u64>,
    #[serde(default)]
    x: Option<i64>,
    #[serde(default)]
    y: Option<i64>,
    #[serde(default)]
    width: Option<i64>,
    #[serde(default)]
    height: Option<i64>,
    #[serde(default)]
    pixel_bytes: Option<u64>,
    #[serde(default)]
    output_bytes: Option<u64>,
    #[serde(default)]
    elapsed_ms: Option<u64>,
}

struct CaptureOutputGuard<'a> {
    path: &'a Path,
    keep: bool,
}

impl<'a> CaptureOutputGuard<'a> {
    fn new(path: &'a Path) -> Self {
        Self { path, keep: false }
    }

    fn preserve(&mut self) {
        self.keep = true;
    }
}

impl Drop for CaptureOutputGuard<'_> {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

pub fn discover_windows_capture_binary() -> Result<PathBuf> {
    let override_path = ["BLUEY_CAPTURE_BIN", "CUE_CAPTURE_BIN"]
        .into_iter()
        .find_map(|name| env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from);
    let current_exe = env::current_exe().context("locate current Bluey executable")?;
    let cwd = cfg!(debug_assertions)
        .then(env::current_dir)
        .transpose()
        .context("locate Bluey development directory")?;
    discover_windows_capture_binary_with(
        cfg!(debug_assertions),
        override_path.as_deref(),
        &current_exe,
        cwd.as_deref(),
    )
}

fn discover_windows_capture_binary_with(
    dev_mode: bool,
    override_path: Option<&Path>,
    current_exe: &Path,
    cwd: Option<&Path>,
) -> Result<PathBuf> {
    let canonical_exe = current_exe
        .canonicalize()
        .with_context(|| format!("canonicalize current executable {}", current_exe.display()))?;
    let install_dir = canonical_exe
        .parent()
        .context("current Bluey executable has no install directory")?
        .canonicalize()
        .context("canonicalize Bluey install directory")?;
    if dev_mode {
        if let Some(path) = override_path {
            return canonical_capture_helper(path);
        }
    } else if override_path.is_some() {
        tracing::warn!("Windows capture helper override ignored in production");
    }

    let mut candidates = Vec::new();
    push_helper_candidates(&mut candidates, &install_dir);
    if dev_mode {
        push_helper_candidates(&mut candidates, &install_dir.join("bin"));
    }
    if dev_mode {
        let cwd = cwd.context("Bluey development directory unavailable")?;
        push_helper_candidates(
            &mut candidates,
            &cwd.join("native/windows/cue-capture/build"),
        );
        push_helper_candidates(&mut candidates, cwd);
    }

    let candidate = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| match dev_mode {
            true => anyhow!(
                "native Windows capture helper not found; build it or set BLUEY_CAPTURE_BIN"
            ),
            false => anyhow!("packaged Windows capture helper not found; reinstall Bluey"),
        })?;
    let canonical = canonical_capture_helper(&candidate)?;
    if !dev_mode {
        verify_packaged_capture_helper(&canonical, &install_dir)?;
    }
    Ok(canonical)
}

pub fn capture_windows_screen(
    output_path: &Path,
    region: Option<WindowsCaptureRegion>,
) -> Result<WindowsCaptureDiagnostic> {
    let helper = discover_windows_capture_binary()?;
    run_windows_capture_helper(&helper, output_path, region, CAPTURE_TIMEOUT)
}

fn push_helper_candidates(candidates: &mut Vec<PathBuf>, directory: &Path) {
    candidates.extend(
        WINDOWS_CAPTURE_HELPER_NAMES
            .iter()
            .map(|name| directory.join(name)),
    );
}

fn canonical_capture_helper(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("Windows capture helper path must be absolute");
    }
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize Windows capture helper {}", path.display()))?;
    if !canonical.is_file() {
        bail!("Windows capture helper path is not a file");
    }
    Ok(canonical)
}

fn verify_packaged_capture_helper(helper: &Path, install_dir: &Path) -> Result<()> {
    let canonical_helper = canonical_capture_helper(helper)?;
    let canonical_install = install_dir.canonicalize().with_context(|| {
        format!(
            "canonicalize Bluey install directory {}",
            install_dir.display()
        )
    })?;
    if !canonical_helper.starts_with(&canonical_install) {
        bail!("Windows capture helper is outside the packaged Bluey install directory");
    }
    let trusted_name = canonical_helper
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            WINDOWS_CAPTURE_HELPER_NAMES
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
        });
    if !trusted_name {
        bail!("Windows capture helper does not use a trusted packaged alias");
    }
    Ok(())
}

fn run_windows_capture_helper(
    helper: &Path,
    output_path: &Path,
    region: Option<WindowsCaptureRegion>,
    timeout: Duration,
) -> Result<WindowsCaptureDiagnostic> {
    if !output_path.is_absolute() {
        bail!("Windows capture output path must be absolute");
    }
    if output_path.extension().and_then(|value| value.to_str()) != Some("png") {
        bail!("Windows capture output must use the .png extension");
    }
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create capture directory {}", parent.display()))?;
    }
    match std::fs::remove_file(output_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("remove stale capture {}", output_path.display()));
        }
    }
    let mut output_guard = CaptureOutputGuard::new(output_path);

    let mut command = Command::new(helper);
    command
        .env_clear()
        .args(capture_helper_arguments(output_path, region))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for name in WINDOWS_CAPTURE_ENV_ALLOWLIST {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("launch Windows capture helper {}", helper.display()))?;
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Windows capture helper stderr pipe unavailable");
        }
    };
    let diagnostics = thread::spawn(move || drain_bounded_diagnostics(stderr));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = diagnostics.join();
                return Err(error).context("poll Windows capture helper");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = diagnostics.join();
            bail!(
                "Windows capture helper timed out after {} ms",
                timeout.as_millis()
            );
        }
        thread::sleep(CAPTURE_POLL_INTERVAL);
    };
    let diagnostic_bytes = diagnostics
        .join()
        .map_err(|_| anyhow!("Windows capture diagnostic reader panicked"))?;
    let event = parse_helper_event(&diagnostic_bytes);

    if !status.success() {
        return Err(capture_failure(status.code(), event.as_ref()));
    }
    let event = event.context("Windows capture helper returned no structured diagnostic")?;
    let diagnostic = validate_success_event(event)?;
    let metadata = std::fs::metadata(output_path)
        .with_context(|| format!("read capture output {}", output_path.display()))?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_CAPTURE_OUTPUT_BYTES
        || metadata.len() != diagnostic.output_bytes
    {
        bail!("Windows capture helper produced an invalid output size");
    }
    output_guard.preserve();
    Ok(diagnostic)
}

fn capture_helper_arguments(
    output_path: &Path,
    region: Option<WindowsCaptureRegion>,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("--screenshot"),
        output_path.as_os_str().to_os_string(),
    ];
    if let Some(region) = region {
        arguments.extend([
            OsString::from("--region"),
            OsString::from(region.x.to_string()),
            OsString::from(region.y.to_string()),
            OsString::from(region.width.to_string()),
            OsString::from(region.height.to_string()),
        ]);
    }
    arguments
}

fn drain_bounded_diagnostics(mut stderr: impl Read) -> Vec<u8> {
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let count = match stderr.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        if captured.len() < MAX_DIAGNOSTIC_BYTES {
            let remaining = MAX_DIAGNOSTIC_BYTES - captured.len();
            captured.extend_from_slice(&buffer[..count.min(remaining)]);
        }
    }
    captured
}

fn parse_helper_event(bytes: &[u8]) -> Option<CaptureHelperEvent> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| serde_json::from_str(line.trim()).ok())
}

fn validate_success_event(event: CaptureHelperEvent) -> Result<WindowsCaptureDiagnostic> {
    if event.event != "captured" {
        bail!("Windows capture helper did not report a captured event");
    }
    let x = i32::try_from(event.x.context("capture diagnostic missing x")?)
        .context("capture diagnostic x is outside range")?;
    let y = i32::try_from(event.y.context("capture diagnostic missing y")?)
        .context("capture diagnostic y is outside range")?;
    let width = i32::try_from(event.width.context("capture diagnostic missing width")?)
        .context("capture diagnostic width is outside range")?;
    let height = i32::try_from(event.height.context("capture diagnostic missing height")?)
        .context("capture diagnostic height is outside range")?;
    let expected_pixel_bytes = checked_capture_pixel_bytes(width, height)?;
    let pixel_bytes = event
        .pixel_bytes
        .context("capture diagnostic missing pixel_bytes")?;
    if pixel_bytes != expected_pixel_bytes {
        bail!("Windows capture helper reported inconsistent pixel bounds");
    }
    let output_bytes = event
        .output_bytes
        .context("capture diagnostic missing output_bytes")?;
    if output_bytes == 0 || output_bytes > MAX_CAPTURE_OUTPUT_BYTES {
        bail!("Windows capture helper reported an invalid output byte count");
    }
    Ok(WindowsCaptureDiagnostic {
        x,
        y,
        width,
        height,
        pixel_bytes,
        output_bytes,
        elapsed_ms: event.elapsed_ms.unwrap_or_default(),
    })
}

fn checked_capture_pixel_bytes(width: i32, height: i32) -> Result<u64> {
    if width <= 0 || height <= 0 {
        bail!("capture width and height must be positive");
    }
    let pixels = u64::try_from(width)
        .unwrap_or_default()
        .checked_mul(u64::try_from(height).unwrap_or_default())
        .context("capture pixel count overflow")?;
    if pixels > MAX_CAPTURE_PIXELS {
        bail!("capture region exceeds Bluey's pixel limit");
    }
    let bytes = pixels
        .checked_mul(CAPTURE_BYTES_PER_PIXEL)
        .context("capture byte count overflow")?;
    if bytes > MAX_CAPTURE_PIXEL_BYTES {
        bail!("capture region exceeds Bluey's byte limit");
    }
    Ok(bytes)
}

fn capture_failure(exit_code: Option<i32>, event: Option<&CaptureHelperEvent>) -> anyhow::Error {
    if let Some(event) = event.filter(|event| event.event == "error") {
        return anyhow!(
            "Windows capture helper failed: code={}, recoverable={}, win32_error={}, exit_code={}",
            event.code.as_deref().unwrap_or("unknown"),
            event.recoverable.unwrap_or(false),
            event.win32_error.unwrap_or_default(),
            exit_code.unwrap_or(-1)
        );
    }
    anyhow!(
        "Windows capture helper failed without a valid diagnostic (exit_code={})",
        exit_code.unwrap_or(-1)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_capture_tree(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "bluey-windows-capture-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn region_contract_supports_negative_monitor_coordinates() {
        assert_eq!(
            WindowsCaptureRegion::from_values([-1920, -1080, 1920, 1080]).unwrap(),
            WindowsCaptureRegion {
                x: -1920,
                y: -1080,
                width: 1920,
                height: 1080,
            }
        );
    }

    #[test]
    fn region_contract_rejects_invalid_sizes_and_ranges() {
        assert!(WindowsCaptureRegion::from_values([0, 0, 0, 1080]).is_err());
        assert!(WindowsCaptureRegion::from_values([0, 0, 10_001, 10_000]).is_err());
        assert!(WindowsCaptureRegion::from_values([i64::from(i32::MAX) + 1, 0, 100, 100]).is_err());
    }

    #[test]
    fn captured_diagnostic_is_strictly_bounded() {
        let event = parse_helper_event(
            br#"{"event":"captured","format":"png","x":-1920,"y":0,"width":1920,"height":1080,"pixel_bytes":8294400,"output_bytes":1200000,"elapsed_ms":42}"#,
        )
        .unwrap();
        let diagnostic = validate_success_event(event).unwrap();
        assert_eq!(diagnostic.x, -1920);
        assert_eq!(diagnostic.pixel_bytes, 8_294_400);

        let invalid = parse_helper_event(
            br#"{"event":"captured","x":0,"y":0,"width":1920,"height":1080,"pixel_bytes":10,"output_bytes":1200000}"#,
        )
        .unwrap();
        assert!(validate_success_event(invalid).is_err());
    }

    #[test]
    fn structured_failure_does_not_echo_paths_or_raw_output() {
        let event = parse_helper_event(
            br#"{"event":"error","code":"desktop_copy_failed","recoverable":true,"win32_error":5}"#,
        )
        .unwrap();
        let message = capture_failure(Some(10), Some(&event)).to_string();
        assert!(message.contains("desktop_copy_failed"));
        assert!(message.contains("win32_error=5"));
        assert!(!message.contains("Users"));
        assert!(!message.contains("capture.png"));
    }

    #[test]
    fn production_discovery_ignores_override_and_uses_packaged_helper() {
        let root = isolated_capture_tree("release-override");
        let install = root.join("install");
        let outside = root.join("outside");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let current_exe = install.join("bluey-daemon.exe");
        let packaged = install.join("screen-driver.exe");
        let override_helper = outside.join("screen-driver.exe");
        std::fs::write(&current_exe, b"daemon").unwrap();
        std::fs::write(&packaged, b"packaged").unwrap();
        std::fs::write(&override_helper, b"override").unwrap();

        let discovered =
            discover_windows_capture_binary_with(false, Some(&override_helper), &current_exe, None)
                .unwrap();
        assert_eq!(discovered, packaged.canonicalize().unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn production_verification_rejects_helper_outside_install_directory() {
        let root = isolated_capture_tree("containment");
        let install = root.join("install");
        let outside = root.join("outside");
        std::fs::create_dir_all(&install).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let helper = outside.join("screen-driver.exe");
        std::fs::write(&helper, b"outside").unwrap();

        let error = verify_packaged_capture_helper(&helper, &install)
            .expect_err("outside helper must be rejected");
        assert!(error.to_string().contains("outside the packaged"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn capture_helper_environment_allowlist_excludes_secrets_and_path() {
        assert_eq!(
            WINDOWS_CAPTURE_ENV_ALLOWLIST,
            &["SystemRoot", "WINDIR", "TEMP", "TMP"]
        );
        for forbidden in [
            "PATH",
            "BLUEY_CAPTURE_BIN",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
        ] {
            assert!(!WINDOWS_CAPTURE_ENV_ALLOWLIST
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(forbidden)));
        }
    }

    #[test]
    fn helper_argument_contract_preserves_negative_coordinates_and_order() {
        assert_eq!(
            capture_helper_arguments(
                Path::new(r"C:\Bluey\capture.png"),
                Some(WindowsCaptureRegion {
                    x: -1920,
                    y: -1080,
                    width: 1920,
                    height: 1080,
                }),
            ),
            [
                "--screenshot",
                r"C:\Bluey\capture.png",
                "--region",
                "-1920",
                "-1080",
                "1920",
                "1080",
            ]
            .map(OsString::from)
        );
    }

    #[cfg(unix)]
    #[test]
    fn invalid_success_diagnostic_removes_untrusted_output() {
        use std::os::unix::fs::PermissionsExt;

        let root = isolated_capture_tree("invalid-success-cleanup");
        std::fs::create_dir_all(&root).unwrap();
        let helper = root.join("fake-capture");
        let output = root.join("capture.png");
        std::fs::write(
            &helper,
            "#!/bin/sh\nprintf x > \"$2\"\nprintf '%s\\n' '{\"event\":\"captured\",\"x\":0,\"y\":0,\"width\":1,\"height\":1,\"pixel_bytes\":5,\"output_bytes\":1}' >&2\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let error = run_windows_capture_helper(&helper, &output, None, Duration::from_secs(1))
            .expect_err("invalid helper diagnostic must fail closed");
        assert!(error.to_string().contains("inconsistent pixel bounds"));
        assert!(!output.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn launcher_rejects_relative_output_before_process_start() {
        let error = run_windows_capture_helper(
            Path::new("/definitely/not/launched"),
            Path::new("capture.png"),
            None,
            Duration::from_millis(1),
        )
        .expect_err("relative capture output must be rejected");
        assert!(error.to_string().contains("must be absolute"));
    }
}
