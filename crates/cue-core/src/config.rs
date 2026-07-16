use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::app_paths::{create_private_file_new, AppPaths};
use crate::clock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub provider: String,
    pub api_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_account_id: Option<String>,
    pub user_id: String,
    pub workspace_id: String,
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub credential_generation: u64,
    pub linked_at: String,
}

impl AccountConfig {
    pub fn local() -> Self {
        Self {
            provider: "local".to_string(),
            api_url: "http://127.0.0.1:8787".to_string(),
            cloud_account_id: None,
            user_id: "local-user".to_string(),
            workspace_id: "default".to_string(),
            device_id: "local-device".to_string(),
            access_token: None,
            refresh_token: None,
            credential_generation: 0,
            linked_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn token_configured(&self) -> bool {
        self.access_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty())
    }

    /// Stable owner identity for credentials embedded in this profile.
    pub fn owner_account_id(&self) -> Option<&str> {
        self.owner_account_id_with_token_state(self.token_configured())
    }

    /// Stable owner identity when credentials stored outside this profile have
    /// already been validated by the caller.
    pub fn owner_account_id_with_token_state(&self, valid_tokens: bool) -> Option<&str> {
        if !valid_tokens || self.provider.trim().eq_ignore_ascii_case("local") {
            return None;
        }

        self.cloud_account_id
            .as_deref()
            .map(str::trim)
            .filter(|account_id| !account_id.is_empty())
            .or_else(|| {
                let user_id = self.user_id.trim();
                (!user_id.is_empty() && user_id != "local-user").then_some(user_id)
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CueSettings {
    pub default_model: String,
    pub default_mode: String,
    pub answer_style: Option<String>,
    pub overlay_opacity: f32,
    pub audio_system_enabled: bool,
    pub audio_microphone_enabled: bool,
    pub cloud_sync_enabled: bool,
    #[serde(default)]
    pub cloud_sync_consent_granted: bool,
    pub retention_days: u32,
    pub updated_at: String,

    /// Codex Stage 24: has the customer been asked once whether they
    /// want auto-disguise during meeting apps?
    #[serde(default)]
    pub auto_disguise_prompted: bool,
    /// Codex Stage 24: customer accepted auto-disguise during meeting apps.
    #[serde(default)]
    pub auto_disguise_enabled: bool,
    /// Codex Stage 24: persisted disguise mode (none / activity / terminal / settings).
    #[serde(default = "default_disguise_mode")]
    pub disguise_mode: String,
    /// Consent-first policy for periodic work-context capture. Starting and
    /// stopping capture remains an explicit runtime action; these fields only
    /// constrain what an approved capture session may observe and retain.
    #[serde(default)]
    pub context_watch: ContextWatchSettings,
    /// Meeting applications explicitly ignored from the detection banner.
    /// This stores process/bundle identities only, never window titles,
    /// transcripts, URLs, or audio data.
    #[serde(default = "default_meeting_detection_enabled")]
    pub meeting_detection_enabled: bool,
    /// Meeting applications explicitly ignored from the detection banner.
    /// This stores process/bundle identities only, never window titles,
    /// transcripts, URLs, or audio data.
    #[serde(default)]
    pub meeting_detection_ignored_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextWatchSettings {
    #[serde(default = "default_context_watch_semantic_first")]
    pub semantic_first: bool,
    #[serde(default = "default_context_watch_screenshot_fallback")]
    pub screenshot_fallback: bool,
    #[serde(default = "default_context_watch_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_context_watch_max_items")]
    pub max_local_items: usize,
    #[serde(default)]
    pub excluded_apps: Vec<String>,
    #[serde(default)]
    pub excluded_domains: Vec<String>,
}

impl Default for ContextWatchSettings {
    fn default() -> Self {
        Self {
            semantic_first: true,
            screenshot_fallback: default_context_watch_screenshot_fallback(),
            interval_secs: default_context_watch_interval_secs(),
            max_local_items: default_context_watch_max_items(),
            excluded_apps: Vec::new(),
            excluded_domains: Vec::new(),
        }
    }
}

impl ContextWatchSettings {
    fn normalize(&mut self) {
        self.interval_secs = self.interval_secs.clamp(3, 300);
        self.max_local_items = self.max_local_items.clamp(10, 500);
        normalize_exclusion_list(&mut self.excluded_apps);
        normalize_exclusion_list(&mut self.excluded_domains);
    }

    pub fn excludes_app(&self, app_name_or_id: &str) -> bool {
        exclusion_matches(&self.excluded_apps, app_name_or_id)
    }

    pub fn excludes_domain(&self, domain_or_url: &str) -> bool {
        exclusion_matches(&self.excluded_domains, domain_or_url)
    }
}

impl Default for CueSettings {
    fn default() -> Self {
        Self {
            default_model: "Bluey Auto".to_string(),
            default_mode: "General".to_string(),
            answer_style: None,
            overlay_opacity: 0.92,
            audio_system_enabled: true,
            audio_microphone_enabled: true,
            cloud_sync_enabled: false,
            cloud_sync_consent_granted: false,
            retention_days: 30,
            updated_at: clock::now_epoch_ms_string(),
            auto_disguise_prompted: false,
            auto_disguise_enabled: false,
            disguise_mode: "none".to_string(),
            context_watch: ContextWatchSettings::default(),
            meeting_detection_enabled: default_meeting_detection_enabled(),
            meeting_detection_ignored_apps: Vec::new(),
        }
    }
}

impl CueSettings {
    /// Whether cloud processing is allowed by the persisted user settings.
    ///
    /// Callers must require both switches. The operational switch cannot be
    /// used as a substitute for the user's explicit cloud-processing consent.
    pub fn cloud_sync_allowed(&self) -> bool {
        self.cloud_sync_enabled && self.cloud_sync_consent_granted
    }

    fn enforce_consent(&mut self) {
        if !self.cloud_sync_allowed() {
            self.cloud_sync_enabled = false;
        }
    }

    pub fn touch(&mut self) {
        self.enforce_consent();
        self.overlay_opacity = self.overlay_opacity.clamp(0.18, 1.0);
        self.retention_days = self.retention_days.clamp(1, 3650);
        self.context_watch.normalize();
        normalize_exclusion_list(&mut self.meeting_detection_ignored_apps);
        self.updated_at = clock::now_epoch_ms_string();
    }
}

const fn default_context_watch_semantic_first() -> bool {
    true
}

const fn default_context_watch_screenshot_fallback() -> bool {
    false
}

const fn default_context_watch_interval_secs() -> u64 {
    12
}

const fn default_context_watch_max_items() -> usize {
    120
}

const fn default_meeting_detection_enabled() -> bool {
    true
}

fn normalize_exclusion_list(values: &mut Vec<String>) {
    let mut normalized = values
        .drain(..)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized.truncate(128);
    *values = normalized;
}

fn exclusion_matches(exclusions: &[String], candidate: &str) -> bool {
    let candidate = candidate.trim().to_ascii_lowercase();
    !candidate.is_empty()
        && exclusions.iter().any(|excluded| {
            candidate.match_indices(excluded).any(|(start, matched)| {
                let end = start + matched.len();
                let before_is_boundary = start == 0
                    || candidate[..start]
                        .chars()
                        .next_back()
                        .is_some_and(is_exclusion_boundary);
                let after_is_boundary = end == candidate.len()
                    || candidate[end..]
                        .chars()
                        .next()
                        .is_some_and(is_exclusion_boundary);
                before_is_boundary && after_is_boundary
            })
        })
}

fn is_exclusion_boundary(value: char) -> bool {
    matches!(value, '.' | '/' | ':' | ' ' | '@' | '?' | '#' | '\\')
}

pub fn load_account(paths: &AppPaths) -> Result<Option<AccountConfig>> {
    match fs::read(&paths.account_file) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", paths.account_file.display()))
            .map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read {}", paths.account_file.display()))
        }
    }
}

pub fn save_account(paths: &AppPaths, account: &AccountConfig) -> Result<()> {
    paths.ensure()?;
    let _lock = lock_account_file(paths)?;
    let current_generation = load_account(paths)?
        .map(|current| current.credential_generation)
        .unwrap_or_default();
    write_next_account_generation(paths, account, current_generation)
}

/// Save an account only if no account/profile write has happened since the
/// caller loaded `expected_generation`.
pub fn save_account_if_generation(
    paths: &AppPaths,
    expected_generation: u64,
    account: &AccountConfig,
) -> Result<bool> {
    paths.ensure()?;
    let _lock = lock_account_file(paths)?;
    let current_generation = load_account(paths)?
        .map(|current| current.credential_generation)
        .unwrap_or_default();
    if current_generation != expected_generation {
        return Ok(false);
    }
    write_next_account_generation(paths, account, current_generation)?;
    Ok(true)
}

pub fn load_settings(paths: &AppPaths) -> Result<CueSettings> {
    match fs::read(&paths.settings_file) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", paths.settings_file.display()))
            .map(|mut settings: CueSettings| {
                settings.enforce_consent();
                settings
            }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(CueSettings::default()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read {}", paths.settings_file.display()))
        }
    }
}

pub fn save_settings(paths: &AppPaths, settings: &CueSettings) -> Result<()> {
    paths.ensure()?;
    let _lock = lock_private_file(&paths.settings_file)?;
    write_private_json_atomic(&paths.settings_file, settings)
}

/// Mutate the latest settings snapshot under one cross-process lock and
/// atomically publish the normalized result.
///
/// Prefer this to a separate `load_settings` / `save_settings` pair so a
/// daemon, dashboard, or CLI process cannot overwrite unrelated fields that
/// another process saved between the read and write.
pub fn update_settings<F>(paths: &AppPaths, update: F) -> Result<CueSettings>
where
    F: FnOnce(&mut CueSettings),
{
    paths.ensure()?;
    let _lock = lock_private_file(&paths.settings_file)?;
    let mut settings = load_settings(paths)?;
    update(&mut settings);
    settings.touch();
    write_private_json_atomic(&paths.settings_file, &settings)?;
    Ok(settings)
}

fn write_next_account_generation(
    paths: &AppPaths,
    account: &AccountConfig,
    current_generation: u64,
) -> Result<()> {
    let mut account = account.clone();
    account.credential_generation = current_generation
        .checked_add(1)
        .context("account credential generation exhausted")?;
    write_private_json_atomic(&paths.account_file, &account)
}

fn lock_account_file(paths: &AppPaths) -> Result<File> {
    lock_private_file(&paths.account_file)
}

fn lock_private_file(path: &Path) -> Result<File> {
    let lock_path = private_lock_path(path);
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open {}", lock_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", lock_path.display()))?;
    }

    lock.lock()
        .with_context(|| format!("failed to lock {}", lock_path.display()))?;
    Ok(lock)
}

fn private_lock_path(path: &Path) -> PathBuf {
    let mut path = path.as_os_str().to_os_string();
    path.push(".lock");
    PathBuf::from(path)
}

fn write_private_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    let bytes = serde_json::to_vec_pretty(value)?;
    let attempt = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut temp_path = path.as_os_str().to_os_string();
    temp_path.push(format!(".tmp-{}-{attempt}", std::process::id()));
    let temp_path = PathBuf::from(temp_path);

    let result = (|| -> Result<()> {
        let mut file = create_private_file_new(&temp_path)?;

        file.write_all(&bytes)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp_path.display()))?;
        drop(file);
        atomic_replace_file(&temp_path, path).with_context(|| {
            format!(
                "failed to replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .with_context(|| format!("failed to sync {}", parent.display()))?;
        }
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(unix)]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn default_disguise_mode() -> String {
    "none".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(name: &str) -> AppPaths {
        let base =
            std::env::temp_dir().join(format!("bluey-config-{name}-{}", uuid::Uuid::new_v4()));
        AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        }
    }

    #[test]
    fn owner_account_id_requires_tokens_and_prefers_cloud_account_id() {
        let mut account = AccountConfig::local();
        account.access_token = Some("access".to_string());
        assert_eq!(account.owner_account_id(), None);

        account.provider = "bluey".to_string();
        account.user_id = " user@example.com ".to_string();
        account.access_token = None;
        assert_eq!(account.owner_account_id(), None);
        assert_eq!(
            account.owner_account_id_with_token_state(true),
            Some("user@example.com")
        );

        account.access_token = Some("access".to_string());
        assert_eq!(account.owner_account_id(), Some("user@example.com"));
        account.cloud_account_id = Some(" account-123 ".to_string());
        assert_eq!(account.owner_account_id(), Some("account-123"));
    }

    #[test]
    fn malformed_account_is_diagnosable_and_cannot_be_overwritten() {
        let paths = test_paths("malformed");
        paths.ensure().unwrap();
        let malformed = br#"{"provider":"bluey","user_id": nope}"#;
        fs::write(&paths.account_file, malformed).unwrap();

        let load_error = load_account(&paths).unwrap_err().to_string();
        assert!(load_error.contains("failed to parse"));
        assert!(load_error.contains(paths.account_file.to_str().unwrap()));

        let mut replacement = AccountConfig::local();
        replacement.provider = "bluey".to_string();
        replacement.user_id = "replacement@example.com".to_string();
        replacement.access_token = Some("replacement-access".to_string());
        let save_error = save_account(&paths, &replacement).unwrap_err().to_string();
        assert!(save_error.contains("failed to parse"));
        assert_eq!(fs::read(&paths.account_file).unwrap(), malformed);

        let _ = fs::remove_dir_all(&paths.config_dir);
    }

    #[test]
    fn stale_account_generation_cannot_overwrite_new_link() {
        let paths = test_paths("generation");
        let mut first = AccountConfig::local();
        first.provider = "bluey".to_string();
        first.cloud_account_id = Some("account-a".to_string());
        first.user_id = "a@example.com".to_string();
        first.access_token = Some("access-a".to_string());
        first.refresh_token = Some("refresh-a".to_string());
        save_account(&paths, &first).unwrap();

        let mut stale = load_account(&paths).unwrap().unwrap();
        let stale_generation = stale.credential_generation;

        let mut linked = AccountConfig::local();
        linked.provider = "bluey".to_string();
        linked.cloud_account_id = Some("account-b".to_string());
        linked.user_id = "b@example.com".to_string();
        linked.access_token = Some("access-b".to_string());
        linked.refresh_token = Some("refresh-b".to_string());
        save_account(&paths, &linked).unwrap();

        stale.access_token = Some("late-access-a".to_string());
        stale.refresh_token = Some("late-refresh-a".to_string());
        assert!(!save_account_if_generation(&paths, stale_generation, &stale).unwrap());

        let current = load_account(&paths).unwrap().unwrap();
        assert_eq!(current.cloud_account_id.as_deref(), Some("account-b"));
        assert_eq!(current.user_id, "b@example.com");
        assert_eq!(current.access_token.as_deref(), Some("access-b"));
        assert!(current.credential_generation > stale_generation);

        let _ = fs::remove_dir_all(&paths.config_dir);
    }

    #[test]
    fn account_publication_uses_owner_only_file_permissions() {
        let paths = test_paths("private-account");
        let mut account = AccountConfig::local();
        account.provider = "bluey".to_string();
        account.user_id = "private@example.com".to_string();
        account.access_token = Some("private-access".to_string());
        account.refresh_token = Some("private-refresh".to_string());

        save_account(&paths, &account).expect("save account");
        crate::app_paths::validate_private_file(&paths.account_file)
            .expect("account file should be owner-only");

        let _ = fs::remove_dir_all(&paths.config_dir);
    }

    #[test]
    fn default_settings_require_cloud_sync_opt_in() {
        let settings = CueSettings::default();

        assert!(!settings.cloud_sync_enabled);
        assert!(!settings.cloud_sync_consent_granted);
        assert_eq!(settings.disguise_mode, "none");
        assert!(settings.context_watch.semantic_first);
        assert!(!settings.context_watch.screenshot_fallback);
        assert_eq!(settings.context_watch.interval_secs, 12);
        assert!(settings.meeting_detection_enabled);
        assert!(settings.meeting_detection_ignored_apps.is_empty());
    }

    #[test]
    fn legacy_settings_keep_meeting_suggestions_enabled_until_user_disables_them() {
        let mut value = serde_json::to_value(CueSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("meeting_detection_enabled");
        let legacy: CueSettings = serde_json::from_value(value).unwrap();
        assert!(legacy.meeting_detection_enabled);

        let mut disabled = CueSettings {
            meeting_detection_enabled: false,
            ..CueSettings::default()
        };
        disabled.touch();
        assert!(!disabled.meeting_detection_enabled);
    }

    #[test]
    fn context_watch_policy_is_backward_compatible_bounded_and_excludable() {
        let mut value = serde_json::to_value(CueSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("context_watch");
        let legacy: CueSettings = serde_json::from_value(value).unwrap();
        assert_eq!(legacy.context_watch, ContextWatchSettings::default());

        let mut settings = CueSettings::default();
        settings.context_watch.interval_secs = 1;
        settings.context_watch.max_local_items = usize::MAX;
        settings.context_watch.excluded_apps = vec![
            "  com.example.Secret  ".to_string(),
            "com.example.secret".to_string(),
        ];
        settings.context_watch.excluded_domains = vec!["Accounts.Example.com".to_string()];
        settings.touch();

        assert_eq!(settings.context_watch.interval_secs, 3);
        assert_eq!(settings.context_watch.max_local_items, 500);
        assert_eq!(
            settings.context_watch.excluded_apps,
            vec!["com.example.secret"]
        );
        assert!(settings
            .context_watch
            .excludes_app("com.example.secret.helper"));
        assert!(settings
            .context_watch
            .excludes_domain("https://accounts.example.com/private"));
        assert!(!settings.context_watch.excludes_domain("example.com"));
    }

    #[test]
    fn legacy_enabled_sync_without_explicit_consent_fails_closed() {
        let mut value = serde_json::to_value(CueSettings::default()).unwrap();
        value["cloud_sync_enabled"] = serde_json::Value::Bool(true);
        value
            .as_object_mut()
            .unwrap()
            .remove("cloud_sync_consent_granted");

        let mut settings: CueSettings = serde_json::from_value(value).unwrap();
        settings.enforce_consent();

        assert!(!settings.cloud_sync_enabled);
        assert!(!settings.cloud_sync_consent_granted);
        assert!(!settings.cloud_sync_allowed());
    }

    #[test]
    fn explicit_sync_consent_stays_enabled() {
        let mut settings = CueSettings {
            cloud_sync_enabled: true,
            cloud_sync_consent_granted: true,
            ..CueSettings::default()
        };

        settings.touch();

        assert!(settings.cloud_sync_enabled);
        assert!(settings.cloud_sync_consent_granted);
        assert!(settings.cloud_sync_allowed());
    }

    #[test]
    fn concurrent_disjoint_settings_updates_preserve_every_field() {
        use std::sync::{Arc, Barrier};

        let paths = test_paths("concurrent-settings");
        save_settings(&paths, &CueSettings::default()).unwrap();
        let paths = Arc::new(paths);
        let barrier = Arc::new(Barrier::new(4));

        let cloud = {
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                update_settings(&paths, |settings| {
                    settings.cloud_sync_consent_granted = true;
                    settings.cloud_sync_enabled = true;
                })
                .unwrap();
            })
        };
        let context = {
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                update_settings(&paths, |settings| {
                    settings.context_watch.interval_secs = 24;
                    settings
                        .context_watch
                        .excluded_domains
                        .push("private.example.com".to_string());
                })
                .unwrap();
            })
        };
        let meetings = {
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                update_settings(&paths, |settings| {
                    settings
                        .meeting_detection_ignored_apps
                        .push("zoom.exe".to_string());
                })
                .unwrap();
            })
        };

        barrier.wait();
        cloud.join().unwrap();
        context.join().unwrap();
        meetings.join().unwrap();

        let settings = load_settings(&paths).unwrap();
        assert!(settings.cloud_sync_enabled);
        assert!(settings.cloud_sync_consent_granted);
        assert_eq!(settings.context_watch.interval_secs, 24);
        assert_eq!(
            settings.context_watch.excluded_domains,
            vec!["private.example.com"]
        );
        assert_eq!(settings.meeting_detection_ignored_apps, vec!["zoom.exe"]);

        let _ = fs::remove_dir_all(&paths.config_dir);
    }
}
