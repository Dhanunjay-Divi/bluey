use std::fs;
use std::io::ErrorKind;

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
    write_private_json(&paths.account_file, account)
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
