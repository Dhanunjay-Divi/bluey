use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::app_paths::AppPaths;
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
    #[serde(default = "legacy_cloud_sync_consent_granted")]
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
        }
    }
}

impl CueSettings {
    fn enforce_consent(&mut self) {
        if !self.cloud_sync_consent_granted {
            self.cloud_sync_enabled = false;
        }
    }

    pub fn touch(&mut self) {
        self.enforce_consent();
        self.overlay_opacity = self.overlay_opacity.clamp(0.18, 1.0);
        self.retention_days = self.retention_days.clamp(1, 3650);
        self.updated_at = clock::now_epoch_ms_string();
    }
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
    write_private_json(&paths.settings_file, settings)
}

fn write_private_json<T: Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

    Ok(())
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
    let lock_path = account_lock_path(&paths.account_file);
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

fn account_lock_path(account_file: &Path) -> PathBuf {
    let mut path = account_file.as_os_str().to_os_string();
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
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", temp_path.display()))?;
        }

        file.write_all(&bytes)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp_path.display()))?;
        drop(file);
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
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

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn default_disguise_mode() -> String {
    "none".to_string()
}

// Existing signed-in installs already enabled saved-session sync under the
// accepted account terms. Missing this newly introduced field must preserve
// that state; brand-new settings still default to false until sign-in.
fn legacy_cloud_sync_consent_granted() -> bool {
    true
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
    fn default_settings_require_cloud_sync_opt_in() {
        let settings = CueSettings::default();

        assert!(!settings.cloud_sync_enabled);
        assert!(!settings.cloud_sync_consent_granted);
        assert_eq!(settings.disguise_mode, "none");
    }

    #[test]
    fn legacy_enabled_sync_without_new_consent_field_stays_on() {
        let mut value = serde_json::to_value(CueSettings::default()).unwrap();
        value["cloud_sync_enabled"] = serde_json::Value::Bool(true);
        value
            .as_object_mut()
            .unwrap()
            .remove("cloud_sync_consent_granted");

        let mut settings: CueSettings = serde_json::from_value(value).unwrap();
        settings.enforce_consent();

        assert!(settings.cloud_sync_enabled);
        assert!(settings.cloud_sync_consent_granted);
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
    }
}
