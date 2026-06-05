use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Url;
use serde::Deserialize;

const DEFAULT_MANIFEST_URL: &str = "https://bluey.sh/latest.json";
const DEFAULT_INSTALL_PATH: &str = "/install.sh";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct ReleaseManifest {
    version: String,
    #[serde(default)]
    platforms: HashMap<String, PlatformArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlatformArtifact {
    url: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
struct UpdatePlan {
    version: String,
    manifest_origin: String,
    install_url: String,
    artifact_url: String,
    artifact_sha256: Option<String>,
    artifact_size_bytes: Option<u64>,
}

pub async fn maybe_update_before_on(title: Option<&str>) -> Result<()> {
    if env_flag("BLUEY_SKIP_UPDATE") {
        return Ok(());
    }
    if running_from_dev_target() && !env_flag("BLUEY_UPDATE_FORCE") {
        return Ok(());
    }

    match check_for_update().await {
        Ok(Some(plan)) => {
            if !confirm_or_auto_update(&plan, false) {
                println!("Bluey update skipped for this launch.");
                return Ok(());
            }
            if let Err(error) = install_update(&plan).await {
                if env_flag("BLUEY_UPDATE_STRICT") {
                    return Err(error);
                }
                eprintln!("warning: Bluey update failed, continuing current version: {error:#}");
                return Ok(());
            }
            relaunch_bluey_on(title)?;
            Ok(())
        }
        Ok(None) => Ok(()),
        Err(error) => {
            if env_flag("BLUEY_UPDATE_STRICT") {
                Err(error)
            } else {
                if env_flag("BLUEY_UPDATE_VERBOSE") {
                    eprintln!("warning: Bluey update check skipped: {error:#}");
                }
                Ok(())
            }
        }
    }
}

pub async fn manual_update(check_only: bool, yes: bool, force: bool) -> Result<()> {
    if running_from_dev_target() && !force && !env_flag("BLUEY_UPDATE_FORCE") {
        println!(
            "Bluey update check skipped for this local dev binary. Re-run with --force to test it."
        );
        return Ok(());
    }

    match check_for_update().await? {
        Some(plan) if check_only => {
            print_update_available(&plan);
            Ok(())
        }
        Some(plan) => {
            print_update_available(&plan);
            if yes || confirm_or_auto_update(&plan, true) {
                install_update(&plan).await?;
                println!(
                    "Bluey updated to {}. Run `bluey on` when ready.",
                    plan.version
                );
            } else {
                println!("Bluey update skipped.");
            }
            Ok(())
        }
        None => {
            println!("Bluey is up to date ({CURRENT_VERSION}).");
            Ok(())
        }
    }
}

async fn check_for_update() -> Result<Option<UpdatePlan>> {
    let manifest_url =
        env::var("BLUEY_UPDATE_MANIFEST_URL").unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_string());
    let manifest_url = Url::parse(&manifest_url)
        .with_context(|| format!("invalid BLUEY_UPDATE_MANIFEST_URL: {manifest_url}"))?;
    let manifest_origin = manifest_origin(&manifest_url)?;
    let install_url = env::var("BLUEY_UPDATE_INSTALL_URL")
        .unwrap_or_else(|_| format!("{manifest_origin}{DEFAULT_INSTALL_PATH}"));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(format!("bluey-cli/{CURRENT_VERSION}"))
        .build()?;
    let response = client
        .get(manifest_url.clone())
        .send()
        .await
        .with_context(|| format!("failed to fetch update manifest from {manifest_url}"))?
        .error_for_status()
        .with_context(|| format!("update manifest returned an error: {manifest_url}"))?;
    let manifest: ReleaseManifest = response
        .json()
        .await
        .with_context(|| format!("update manifest was not valid JSON: {manifest_url}"))?;

    if !is_remote_newer(&manifest.version, CURRENT_VERSION) {
        return Ok(None);
    }

    let (platform, artifact) = select_artifact(&manifest)
        .with_context(|| format!("no supported artifact for {}", current_platform()))?;
    let artifact_url = resolve_artifact_url(&manifest_url, &artifact.url)
        .with_context(|| format!("invalid artifact URL for {platform}: {}", artifact.url))?;

    Ok(Some(UpdatePlan {
        version: manifest.version,
        manifest_origin,
        install_url,
        artifact_url,
        artifact_sha256: artifact.sha256,
        artifact_size_bytes: artifact.size_bytes,
    }))
}

fn print_update_available(plan: &UpdatePlan) {
    let size = plan
        .artifact_size_bytes
        .map(human_size)
        .unwrap_or_else(|| "unknown size".to_string());
    println!(
        "Bluey {} is available (current {CURRENT_VERSION}, {size}).",
        plan.version
    );
}

fn confirm_or_auto_update(plan: &UpdatePlan, manual: bool) -> bool {
    print_update_available(plan);
    if env_flag("BLUEY_UPDATE_ASSUME_YES") {
        return true;
    }
    let intro = if manual {
        "Press Esc within 5 seconds to cancel."
    } else {
        "Press Esc within 5 seconds to skip; otherwise Bluey updates before starting."
    };
    println!("{intro}");
    !wait_for_escape(Duration::from_secs(5))
}

async fn install_update(plan: &UpdatePlan) -> Result<()> {
    let script = download_install_script(&plan.install_url).await?;
    best_effort_stop_running_bluey();

    println!("Updating Bluey to {}...", plan.version);
    let status = Command::new("bash")
        .arg(&script)
        .env("BLUEY_VERSION", &plan.version)
        .env("BLUEY_DOWNLOAD_HOST", &plan.manifest_origin)
        .env("BLUEY_ARTIFACT_URL", &plan.artifact_url)
        .env_remove("BLUEY_SKIP_UPDATE")
        .env_remove("BLUEY_UPDATE_FORCE")
        .envs(
            plan.artifact_sha256
                .as_ref()
                .map(|value| [("BLUEY_ARTIFACT_SHA256", value.as_str())])
                .into_iter()
                .flatten(),
        )
        .status()
        .context("failed to run Bluey installer")?;

    if !status.success() {
        bail!("installer exited with status {status}");
    }
    Ok(())
}

async fn download_install_script(url: &str) -> Result<PathBuf> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("bluey-cli/{CURRENT_VERSION}"))
        .build()?;
    let bytes = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to fetch installer from {url}"))?
        .error_for_status()
        .with_context(|| format!("installer endpoint returned an error: {url}"))?
        .bytes()
        .await
        .context("failed to read installer response")?;

    let text = std::str::from_utf8(&bytes).context("installer was not UTF-8 shell text")?;
    if !text.starts_with("#!/") || !text.contains("Bluey one-line installer") {
        bail!("installer response did not look like Bluey's install.sh");
    }

    let path = env::temp_dir().join(format!(
        "bluey-install-{}-{}.sh",
        std::process::id(),
        unix_timestamp_millis()
    ));
    fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn best_effort_stop_running_bluey() {
    let exe = env::current_exe().ok();
    let mut candidates = Vec::new();
    if let Some(exe) = exe {
        candidates.push(exe);
    }
    candidates.push(PathBuf::from("bluey"));

    for candidate in candidates {
        let _ = Command::new(candidate)
            .arg("off")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn relaunch_bluey_on(title: Option<&str>) -> Result<()> {
    println!("Bluey updated. Restarting Bluey...");
    let mut command = Command::new("bluey");
    command.arg("on");
    if let Some(title) = title.filter(|value| !value.trim().is_empty()) {
        command.arg("--title").arg(title);
    }
    command.env("BLUEY_SKIP_UPDATE", "1");

    #[cfg(unix)]
    {
        Err(command.exec()).context("failed to relaunch updated bluey")
    }
    #[cfg(not(unix))]
    {
        command
            .spawn()
            .context("failed to relaunch updated bluey")?;
        std::process::exit(0);
    }
}

fn select_artifact(manifest: &ReleaseManifest) -> Option<(String, PlatformArtifact)> {
    let platform = current_platform();
    let mut candidates = vec![platform.clone()];
    if platform == "darwin-arm64" {
        candidates.push("darwin-universal".to_string());
    }
    for candidate in candidates {
        if let Some(artifact) = manifest.platforms.get(&candidate) {
            return Some((candidate, artifact.clone()));
        }
    }
    None
}

fn current_platform() -> String {
    match (env::consts::OS, env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-arm64".to_string(),
        ("macos", "x86_64") => "darwin-universal".to_string(),
        ("windows", "x86_64") => "windows-x86_64".to_string(),
        ("linux", "x86_64") => "linux-x86_64".to_string(),
        (os, arch) => format!("{os}-{arch}"),
    }
}

fn resolve_artifact_url(manifest_url: &Url, artifact_url: &str) -> Result<String> {
    Ok(manifest_url.join(artifact_url)?.to_string())
}

fn manifest_origin(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("manifest URL has no host"))?;
    let mut origin = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Ok(origin)
}

fn is_remote_newer(remote: &str, current: &str) -> bool {
    compare_versions(remote, current).is_gt()
}

fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let left = parse_version(left);
    let right = parse_version(right);
    left.cmp(&right)
}

fn parse_version(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|part| {
            part.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

fn running_from_dev_target() -> bool {
    env::current_exe()
        .ok()
        .as_deref()
        .is_some_and(|path| path_has_component(path, "target"))
}

fn path_has_component(path: &Path, component: &str) -> bool {
    path.components()
        .any(|part| part.as_os_str().to_string_lossy() == component)
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn human_size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / MB)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn unix_timestamp_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

#[cfg(unix)]
fn wait_for_escape(timeout: Duration) -> bool {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    use std::time::Instant;

    let stdin = io::stdin();
    if !stdin.is_terminal() {
        std::thread::sleep(timeout);
        return false;
    }
    let fd = stdin.as_raw_fd();
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    let raw_enabled = unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) == 0 };
    let original = if raw_enabled {
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        let enabled = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) == 0 };
        enabled.then_some(original)
    } else {
        None
    };

    let deadline = Instant::now() + timeout;
    let mut cancelled = false;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let poll_ms = remaining.min(Duration::from_millis(100)).as_millis() as i32;
        let mut fds = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut fds, 1, poll_ms) };
        if ready > 0 && (fds.revents & libc::POLLIN) != 0 {
            let mut byte = [0_u8; 1];
            if io::stdin().read(&mut byte).ok() == Some(1) && byte[0] == 0x1b {
                cancelled = true;
                break;
            }
        }
        let _ = io::stdout().flush();
    }

    if let Some(original) = original {
        let _ = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &original) };
    }
    cancelled
}

#[cfg(not(unix))]
fn wait_for_escape(timeout: Duration) -> bool {
    std::thread::sleep(timeout);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_handles_v_prefix_and_patch() {
        assert!(is_remote_newer("v0.2.0", "0.1.9"));
        assert!(is_remote_newer("0.1.1", "0.1.0"));
        assert!(!is_remote_newer("0.1.0", "0.1.0"));
        assert!(!is_remote_newer("0.1.0", "0.1.1"));
    }

    #[test]
    fn resolves_relative_artifact_url_against_manifest() {
        let manifest = Url::parse("https://bluey.sh/latest.json").unwrap();
        assert_eq!(
            resolve_artifact_url(&manifest, "releases/v0.2.0/bluey.tgz").unwrap(),
            "https://bluey.sh/releases/v0.2.0/bluey.tgz"
        );
    }

    #[test]
    fn selects_universal_fallback_for_arm64() {
        let mut manifest = ReleaseManifest {
            version: "9.9.9".to_string(),
            platforms: HashMap::new(),
        };
        manifest.platforms.insert(
            "darwin-universal".to_string(),
            PlatformArtifact {
                url: "releases/v9.9.9/bluey.tgz".to_string(),
                sha256: None,
                size_bytes: None,
            },
        );
        if current_platform() == "darwin-arm64" {
            let (platform, _) = select_artifact(&manifest).unwrap();
            assert_eq!(platform, "darwin-universal");
        }
    }

    #[test]
    fn path_component_detection_is_exact() {
        assert!(path_has_component(
            Path::new("/tmp/project/target/debug/bluey"),
            "target"
        ));
        assert!(!path_has_component(
            Path::new("/tmp/project/not-target/debug/bluey"),
            "target"
        ));
    }
}
