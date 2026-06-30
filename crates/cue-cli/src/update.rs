use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(unix)]
use std::io::{self, IsTerminal, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use reqwest::Url;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DEFAULT_MANIFEST_URL: &str = "https://bluey.sh/latest.json";
const DEFAULT_INSTALL_PATH: &str = "/install.sh";
const DEFAULT_WINDOWS_INSTALL_PATH: &str = "/install.ps1";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct ReleaseManifest {
    version: String,
    #[serde(default)]
    install: Option<InstallScriptArtifact>,
    #[serde(default)]
    windows_install: Option<InstallScriptArtifact>,
    #[serde(default)]
    platforms: HashMap<String, PlatformArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallScriptArtifact {
    url: String,
    #[serde(default)]
    sha256: Option<String>,
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
    install_sha256: Option<String>,
    artifact_url: String,
    artifact_sha256: Option<String>,
    artifact_size_bytes: Option<u64>,
    manifest_trust: ManifestTrust,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManifestTrust {
    Verified,
    UnsignedAllowed(String),
    Unverified(String),
}

impl ManifestTrust {
    fn permits_install(&self) -> bool {
        matches!(self, Self::Verified | Self::UnsignedAllowed(_))
    }

    fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }
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
            if update_check_only_requested() {
                print_update_available(&plan);
                print_update_posture(&plan);
                return Ok(());
            }
            if let Err(error) = ensure_update_installable(&plan) {
                if env_flag("BLUEY_UPDATE_STRICT") {
                    return Err(error);
                }
                eprintln!("warning: Bluey auto-update disabled: {error:#}");
                return Ok(());
            }
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
            print_update_posture(&plan);
            Ok(())
        }
        Some(plan) => {
            print_update_available(&plan);
            ensure_update_installable(&plan)?;
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
    let manifest_bytes = response
        .bytes()
        .await
        .context("failed to read update manifest response")?;
    let manifest_trust = verify_hosted_manifest(&client, &manifest_url, &manifest_bytes).await;
    let manifest: ReleaseManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("update manifest was not valid JSON: {manifest_url}"))?;

    if !is_remote_newer(&manifest.version, CURRENT_VERSION) {
        return Ok(None);
    }

    let install_override = env::var("BLUEY_UPDATE_INSTALL_URL").ok();
    let install_from_manifest = select_install_artifact(&manifest);
    let install_url = if let Some(url) = install_override.as_deref() {
        url.to_string()
    } else if let Some(install) = install_from_manifest {
        resolve_artifact_url(&manifest_url, &install.url)
            .with_context(|| format!("invalid installer URL: {}", install.url))?
    } else {
        format!("{manifest_origin}{}", default_install_path())
    };
    let install_sha256 = install_override
        .is_none()
        .then(|| install_from_manifest.and_then(|install| install.sha256.clone()))
        .flatten();

    let (platform, artifact) = select_artifact(&manifest)
        .with_context(|| format!("no supported artifact for {}", current_platform()))?;
    let artifact_url = resolve_artifact_url(&manifest_url, &artifact.url)
        .with_context(|| format!("invalid artifact URL for {platform}: {}", artifact.url))?;

    Ok(Some(UpdatePlan {
        version: manifest.version,
        manifest_origin,
        install_url,
        install_sha256,
        artifact_url,
        artifact_sha256: artifact.sha256,
        artifact_size_bytes: artifact.size_bytes,
        manifest_trust,
    }))
}

async fn verify_hosted_manifest(
    client: &reqwest::Client,
    manifest_url: &Url,
    manifest_bytes: &[u8],
) -> ManifestTrust {
    let Some(pubkey) = embedded_update_pubkey() else {
        return manifest_trust_from_error("build has no BLUEY_UPDATE_PUBKEY".to_string());
    };
    let signature_url = signature_url_for_manifest(manifest_url);
    let signature_bytes = match client
        .get(signature_url.clone())
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
    {
        Ok(response) => match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                return manifest_trust_from_error(format!(
                    "failed to read signature {signature_url}: {error}"
                ));
            }
        },
        Err(error) => {
            return manifest_trust_from_error(format!(
                "failed to fetch signature {signature_url}: {error}"
            ));
        }
    };

    match verify_manifest_signature(manifest_bytes, signature_bytes.as_ref(), pubkey) {
        Ok(()) => ManifestTrust::Verified,
        Err(error) => manifest_trust_from_error(format!("{error:#}")),
    }
}

fn embedded_update_pubkey() -> Option<&'static str> {
    option_env!("BLUEY_UPDATE_PUBKEY").and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then_some(value)
    })
}

fn manifest_trust_from_error(reason: String) -> ManifestTrust {
    if env_flag("BLUEY_UPDATE_ALLOW_UNSIGNED") {
        ManifestTrust::UnsignedAllowed(reason)
    } else {
        ManifestTrust::Unverified(reason)
    }
}

fn signature_url_for_manifest(manifest_url: &Url) -> String {
    let mut url = manifest_url.clone();
    let path = url.path().trim_end_matches('/');
    url.set_path(&format!("{path}.sig"));
    url.to_string()
}

fn verify_manifest_signature(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    pubkey_b64: &str,
) -> Result<()> {
    let pubkey = BASE64_STANDARD
        .decode(pubkey_b64.trim())
        .context("release public key was not valid base64")?;
    let pubkey: [u8; 32] = pubkey
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("release public key must be 32 raw ed25519 bytes"))?;
    let signature = decode_detached_signature(signature_bytes)?;

    let verifying_key =
        VerifyingKey::from_bytes(&pubkey).context("release public key was not valid ed25519")?;
    let signature = Signature::from_bytes(&signature);
    verifying_key
        .verify(manifest_bytes, &signature)
        .context("release manifest signature verification failed")?;
    Ok(())
}

fn decode_detached_signature(signature_bytes: &[u8]) -> Result<[u8; 64]> {
    if let Ok(text) = std::str::from_utf8(signature_bytes) {
        let text = text.trim();
        if !text.is_empty() {
            if let Ok(decoded) = BASE64_STANDARD.decode(text) {
                return decoded
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow!("release manifest signature must be 64 bytes"));
            }
        }
    }

    signature_bytes
        .try_into()
        .map_err(|_| anyhow!("release manifest signature must be base64 or 64 raw bytes"))
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

fn print_update_posture(plan: &UpdatePlan) {
    match &plan.manifest_trust {
        ManifestTrust::Verified => {
            println!("Run `bluey update` to install when ready.");
        }
        ManifestTrust::UnsignedAllowed(reason) => {
            println!("warning: unsigned update install is enabled for development ({reason}).");
        }
        ManifestTrust::Unverified(reason) => {
            println!(
                "Update install is disabled because the release manifest is not verified: {reason}"
            );
            println!(
                "Release operators must publish latest.json.sig; BLUEY_UPDATE_ALLOW_UNSIGNED=1 is for local testing only."
            );
        }
    }
}

fn ensure_update_installable(plan: &UpdatePlan) -> Result<()> {
    if !plan.manifest_trust.permits_install() {
        bail!(
            "refusing to install unsigned Bluey update; publish latest.json.sig or set BLUEY_UPDATE_ALLOW_UNSIGNED=1 only for local testing"
        );
    }
    if !plan.manifest_trust.is_verified() && !env_flag("BLUEY_UPDATE_ALLOW_UNSIGNED") {
        bail!("refusing to install update without a verified release manifest");
    }
    if plan.install_sha256.is_none() && !env_flag("BLUEY_UPDATE_ALLOW_UNSIGNED") {
        bail!("refusing to install update because latest.json does not pin installer sha256");
    }
    if plan.artifact_sha256.is_none() && !env_flag("BLUEY_UPDATE_ALLOW_UNSIGNED") {
        bail!("refusing to install update because latest.json does not pin artifact sha256");
    }
    Ok(())
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
    let script = download_install_script(&plan.install_url, plan.install_sha256.as_deref()).await?;
    best_effort_stop_running_bluey();

    println!("Updating Bluey to {}...", plan.version);
    let mut command = installer_command(&script);
    let status = command
        .env("BLUEY_VERSION", &plan.version)
        .env("BLUEY_DOWNLOAD_HOST", &plan.manifest_origin)
        .env("BLUEY_ARTIFACT_URL", &plan.artifact_url)
        .env("BLUEY_INSTALL_CONTEXT", "update")
        .env_remove("BLUEY_SKIP_UPDATE")
        .env_remove("BLUEY_UPDATE_FORCE")
        .env_remove("BLUEY_SKIP_CHECKSUM")
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

async fn download_install_script(url: &str, expected_sha256: Option<&str>) -> Result<PathBuf> {
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

    if let Some(expected) = expected_sha256 {
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            bail!("installer sha256 mismatch: expected {expected}, got {actual}");
        }
    } else if !env_flag("BLUEY_UPDATE_ALLOW_UNSIGNED") {
        bail!("installer sha256 missing from signed update manifest");
    }

    let _ = std::str::from_utf8(&bytes).context("installer was not UTF-8 text")?;

    let path = env::temp_dir().join(format!(
        "bluey-install-{}-{}.{}",
        std::process::id(),
        unix_timestamp_millis(),
        installer_extension()
    ));
    fs::write(&path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

fn installer_command(script: &Path) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(script);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = Command::new("bash");
        command.arg(script);
        command
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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
    let mut command = Command::new(relaunch_bluey_bin());
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

fn relaunch_bluey_bin() -> PathBuf {
    relaunch_bluey_bin_from(env::current_exe().ok())
}

fn relaunch_bluey_bin_from(current_exe: Option<PathBuf>) -> PathBuf {
    current_exe
        .filter(|path| !path_has_component(path, "target"))
        .unwrap_or_else(|| PathBuf::from("bluey"))
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

fn select_install_artifact(manifest: &ReleaseManifest) -> Option<&InstallScriptArtifact> {
    if cfg!(windows) {
        manifest
            .windows_install
            .as_ref()
            .or(manifest.install.as_ref())
    } else {
        manifest.install.as_ref()
    }
}

fn default_install_path() -> &'static str {
    if cfg!(windows) {
        DEFAULT_WINDOWS_INSTALL_PATH
    } else {
        DEFAULT_INSTALL_PATH
    }
}

fn installer_extension() -> &'static str {
    if cfg!(windows) {
        "ps1"
    } else {
        "sh"
    }
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
        .is_some_and(running_from_dev_target_path)
}

fn running_from_dev_target_path(path: &Path) -> bool {
    path_has_component(path, "target")
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

fn env_flag_disabled(name: &str) -> bool {
    env::var(name)
        .map(|value| flag_value_is_disabled(&value))
        .unwrap_or(false)
}

fn flag_value_is_disabled(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    matches!(value.as_str(), "0" | "false" | "no" | "off")
}

fn update_check_only_requested() -> bool {
    env_flag("BLUEY_UPDATE_CHECK_ONLY") || env_flag_disabled("BLUEY_AUTO_UPDATE")
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
    use ed25519_dalek::{Signer, SigningKey};

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
            install: None,
            windows_install: None,
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
    fn parses_platform_specific_windows_installer_metadata() {
        let manifest: ReleaseManifest = serde_json::from_str(
            r#"{
              "version": "9.9.9",
              "install": {"url": "install.sh", "sha256": "unix"},
              "windows_install": {"url": "install.ps1", "sha256": "win"},
              "platforms": {}
            }"#,
        )
        .unwrap();

        assert_eq!(manifest.install.unwrap().url, "install.sh");
        assert_eq!(manifest.windows_install.unwrap().sha256.unwrap(), "win");
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

    #[test]
    fn relaunch_prefers_installed_current_exe() {
        assert_eq!(
            relaunch_bluey_bin_from(Some(PathBuf::from("/Users/example/.bluey/bin/bluey"))),
            PathBuf::from("/Users/example/.bluey/bin/bluey")
        );
    }

    #[test]
    fn relaunch_falls_back_to_path_for_dev_target() {
        assert_eq!(
            relaunch_bluey_bin_from(Some(PathBuf::from("/Users/example/cue/target/debug/bluey"))),
            PathBuf::from("bluey")
        );
    }

    #[test]
    fn verifies_valid_release_manifest_signature() {
        let manifest = br#"{"version":"9.9.9","platforms":{}}"#;
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let signature = signing_key.sign(manifest);
        let signature_b64 = BASE64_STANDARD.encode(signature.to_bytes());
        let pubkey_b64 = BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes());

        verify_manifest_signature(manifest, signature_b64.as_bytes(), &pubkey_b64).unwrap();
    }

    #[test]
    fn rejects_tampered_release_manifest_signature() {
        let manifest = br#"{"version":"9.9.9","platforms":{}}"#;
        let tampered = br#"{"version":"9.9.10","platforms":{}}"#;
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let signature = signing_key.sign(manifest);
        let signature_b64 = BASE64_STANDARD.encode(signature.to_bytes());
        let pubkey_b64 = BASE64_STANDARD.encode(signing_key.verifying_key().to_bytes());

        assert!(
            verify_manifest_signature(tampered, signature_b64.as_bytes(), &pubkey_b64).is_err()
        );
    }

    #[test]
    fn rejects_missing_release_manifest_signature() {
        assert!(decode_detached_signature(b"").is_err());
    }

    #[test]
    fn unverified_manifest_is_not_installable_by_default() {
        let plan = UpdatePlan {
            version: "9.9.9".to_string(),
            manifest_origin: "https://bluey.sh".to_string(),
            install_url: "https://bluey.sh/install.sh".to_string(),
            install_sha256: Some("abc".to_string()),
            artifact_url: "https://bluey.sh/releases/v9.9.9/bluey.tgz".to_string(),
            artifact_sha256: Some("def".to_string()),
            artifact_size_bytes: Some(123),
            manifest_trust: ManifestTrust::Unverified("missing signature".to_string()),
        };

        assert!(ensure_update_installable(&plan).is_err());
    }

    #[test]
    fn verified_manifest_still_requires_installer_and_artifact_hashes() {
        let mut plan = UpdatePlan {
            version: "9.9.9".to_string(),
            manifest_origin: "https://bluey.sh".to_string(),
            install_url: "https://bluey.sh/install.sh".to_string(),
            install_sha256: None,
            artifact_url: "https://bluey.sh/releases/v9.9.9/bluey.tgz".to_string(),
            artifact_sha256: Some("def".to_string()),
            artifact_size_bytes: Some(123),
            manifest_trust: ManifestTrust::Verified,
        };
        assert!(ensure_update_installable(&plan).is_err());

        plan.install_sha256 = Some("abc".to_string());
        plan.artifact_sha256 = None;
        assert!(ensure_update_installable(&plan).is_err());

        plan.artifact_sha256 = Some("def".to_string());
        assert!(ensure_update_installable(&plan).is_ok());
    }

    #[test]
    fn installer_sha256_matches_exact_downloaded_script_bytes() {
        let bytes = b"#!/usr/bin/env bash\necho bluey\n";
        assert_eq!(sha256_hex(bytes).len(), 64);
        assert_ne!(
            sha256_hex(bytes),
            sha256_hex(b"#!/usr/bin/env bash\necho other\n")
        );
    }

    #[test]
    fn explicit_disabled_flag_values_disable_auto_update() {
        for value in ["0", "false", "no", "off", " FALSE "] {
            assert!(flag_value_is_disabled(value));
        }
        for value in ["", "1", "true", "yes", "on", "maybe"] {
            assert!(!flag_value_is_disabled(value));
        }
    }
}
