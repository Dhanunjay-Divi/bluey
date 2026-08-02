use std::fs;
use std::io::ErrorKind;
use std::io::Write;
use std::path::{Path, PathBuf};

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
            linked_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn token_configured(&self) -> bool {
        self.access_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty())
    }

    /// Stable non-secret owner identity for local data isolation.
    ///
    /// Authentication secrets may live in the OS credential store, so local
    /// ownership must not depend on bearer-token fields in account.json.
    /// Signing out resets `provider` to `local` before this returns `None`.
    pub fn linked_owner_id(&self) -> Option<&str> {
        if self.provider.trim().is_empty() || self.provider == "local" {
            return None;
        }
        self.cloud_account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
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
            cloud_sync_enabled: true,
            retention_days: 30,
            updated_at: clock::now_epoch_ms_string(),
            auto_disguise_prompted: false,
            auto_disguise_enabled: false,
            disguise_mode: "activity".to_string(),
        }
    }
}

impl CueSettings {
    pub fn touch(&mut self) {
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
    write_private_json(&paths.account_file, account)
}

pub fn load_settings(paths: &AppPaths) -> Result<CueSettings> {
    match fs::read(&paths.settings_file) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", paths.settings_file.display())),
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

fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent", path.display()))?;
    crate::app_paths::create_private_dir(parent)?;
    let temporary = config_temporary_path(path);
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("failed to write {}", temporary.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to set permissions on {}", temporary.display()))?;
        }

        file.sync_all()
            .with_context(|| format!("failed to sync {}", temporary.display()))?;
        atomic_replace_config_file(&temporary, path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        sync_config_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn config_temporary_path(path: &Path) -> PathBuf {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            ".bluey-config-{}-{}.tmp",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ))
}

#[cfg(unix)]
fn atomic_replace_config_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn atomic_replace_config_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let from = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn atomic_replace_config_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(unix)]
fn sync_config_directory(path: &Path) -> Result<()> {
    fs::File::open(path)
        .with_context(|| format!("failed to open {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync {}", path.display()))
}

#[cfg(not(unix))]
fn sync_config_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn default_disguise_mode() -> String {
    "activity".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_enable_cloud_sync_after_sign_in() {
        let settings = CueSettings::default();

        assert!(settings.cloud_sync_enabled);
    }

    #[test]
    fn linked_owner_identity_does_not_require_inline_tokens() {
        let mut account = AccountConfig::local();
        account.provider = "bluey".to_string();
        account.cloud_account_id = Some(" account-1 ".to_string());
        account.user_id = "user@example.com".to_string();
        account.access_token = None;
        account.refresh_token = None;

        assert_eq!(account.linked_owner_id(), Some("account-1"));

        account.provider = "local".to_string();
        assert_eq!(account.linked_owner_id(), None);
    }
}
