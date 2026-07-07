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

    /// Agent bridge: global consent to read other agents' session history.
    /// Off by default — reading a prior agent session requires opt-in.
    #[serde(default)]
    pub allow_agent_session_history: bool,
    /// Agent bridge: the attached coding agent, as a snake_case [`AgentKind`]
    /// label (e.g. "claude_code"). `None` means no agent is attached and
    /// answers route through Bluey's normal providers.
    #[serde(default)]
    pub attached_agent: Option<String>,
    /// Agent bridge: the session id to resume on the attached agent, if the
    /// user attached with a session to continue. `None` means start a fresh
    /// session. Cleared on detach.
    #[serde(default)]
    pub attached_session: Option<String>,
    /// Agent bridge: per-run model override for the attached agent (a vendor
    /// model id, e.g. "composer-2.5" / "gpt-5.1-codex"). Applied via the
    /// registry row's `model_flag`; a no-op for agents with no model flag.
    /// Cleared on detach.
    #[serde(default)]
    pub attached_model: Option<String>,
    /// Agent bridge: whether the attached session has already received the HEAVY
    /// first-turn meeting context (pre-meeting brief, saved summary, back-history
    /// transcript, and Bluey's own Q&A). `false` on a fresh attach / new session;
    /// set `true` after the first answered turn (in the conversation-chaining
    /// persist). While `true`, later turns send only the always-pinned delta
    /// (decisions ledger + recent transcript) plus the new question — never the
    /// heavy blob again. Reset to `false` whenever `attached_session` changes to a
    /// new id, and on detach, so a re-attached session re-primes.
    #[serde(default)]
    pub attached_context_primed: bool,

    /// Agent bridge: vendors for which the user has acknowledged the BYOT
    /// billing disclosure. Each entry is a lowercase `vendor_short` string
    /// from the cloud registry row (e.g. `"anthropic"`, `"codex_cloud"`).
    /// The daemon refuses to mark a BYOT (`BillingModel::ApiCredits`) cloud
    /// agent attached until its vendor appears in this list — that's how the
    /// disclosure modal is unbypassable.
    ///
    /// Empty by default. Once a user acknowledges a vendor's disclosure, the
    /// vendor stays in this list across daemon restarts so they aren't
    /// re-prompted on every launch. Removed when the user detaches and
    /// explicitly clears their stored credential.
    #[serde(default)]
    pub accepted_byot_vendors: Vec<String>,

    /// Overlay: sessions the user has pinned to the top of the redesigned
    /// at-scale session list. Stored as meeting ids; the overlay's session
    /// query sorts pinned-first. Empty by default; persists across restarts.
    #[serde(default)]
    pub pinned_overlay_sessions: Vec<uuid::Uuid>,

    /// Question→trigger (master doc §6): names that mark a spoken line as
    /// addressed to the local user (e.g. `["Alex", "AJ"]`). When *someone else*
    /// asks a question that mentions one of these, Bluey treats it as "this is
    /// for me" and surfaces an answer from the attached agent. Empty disables
    /// name-gated triggering (the loop still works via the manual ask path).
    #[serde(default)]
    pub my_names: Vec<String>,

    /// Question→trigger: when `true`, a detected for-me question fires the
    /// attached agent automatically; when `false` (default), Bluey only
    /// *suggests* (surfaces the detected question as a card) and the user taps
    /// to ask. Opt-in auto mode per the master doc's "suggest by default".
    #[serde(default)]
    pub auto_trigger_enabled: bool,

    /// Live meeting memory (PLAN-CONTEXT-WARMUP SET 0): the decisions ledger +
    /// rolling summary that keep the attached agent current during a meeting.
    /// Extraction runs through a throwaway one-shot drive of the user's OWN
    /// attached agent (never a Bluey-hosted model); the cloud cheap-lane is
    /// only a fallback when one is configured. Default ON — this is the
    /// product's context spine; turn off to stop all background extraction.
    #[serde(default = "default_live_memory_enabled")]
    pub live_memory_enabled: bool,
}

fn default_live_memory_enabled() -> bool {
    true
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
            retention_days: 30,
            updated_at: clock::now_epoch_ms_string(),
            auto_disguise_prompted: false,
            auto_disguise_enabled: false,
            disguise_mode: "activity".to_string(),
            allow_agent_session_history: false,
            attached_agent: None,
            attached_session: None,
            attached_model: None,
            attached_context_primed: false,
            accepted_byot_vendors: Vec::new(),
            pinned_overlay_sessions: Vec::new(),
            my_names: Vec::new(),
            auto_trigger_enabled: false,
            live_memory_enabled: true,
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
    "activity".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_without_agent_fields_still_load() {
        // A config written before the agent-bridge fields existed must still
        // deserialize, defaulting the new fields.
        let legacy = r#"{
            "default_model": "Bluey Auto",
            "default_mode": "General",
            "answer_style": null,
            "overlay_opacity": 0.9,
            "audio_system_enabled": true,
            "audio_microphone_enabled": true,
            "cloud_sync_enabled": false,
            "retention_days": 30,
            "updated_at": "0"
        }"#;
        let settings: CueSettings = serde_json::from_str(legacy).expect("legacy config loads");
        assert!(!settings.allow_agent_session_history);
        assert_eq!(settings.attached_agent, None);
        assert_eq!(settings.attached_session, None);
        assert_eq!(settings.attached_model, None);
    }

    #[test]
    fn agent_fields_roundtrip_through_json() {
        let settings = CueSettings {
            allow_agent_session_history: true,
            attached_agent: Some("claude_code".to_string()),
            attached_session: Some("sess-42".to_string()),
            attached_model: Some("gpt-5.1-codex".to_string()),
            ..CueSettings::default()
        };
        let json = serde_json::to_string(&settings).expect("serialize");
        let parsed: CueSettings = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.allow_agent_session_history);
        assert_eq!(parsed.attached_agent.as_deref(), Some("claude_code"));
        assert_eq!(parsed.attached_session.as_deref(), Some("sess-42"));
        assert_eq!(parsed.attached_model.as_deref(), Some("gpt-5.1-codex"));
    }

    #[test]
    fn legacy_settings_with_agent_but_no_session_still_load() {
        // A config written after `attached_agent` existed but before
        // `attached_session` was added must still deserialize, defaulting the
        // session to `None`.
        let legacy = r#"{
            "default_model": "Bluey Auto",
            "default_mode": "General",
            "answer_style": null,
            "overlay_opacity": 0.9,
            "audio_system_enabled": true,
            "audio_microphone_enabled": true,
            "cloud_sync_enabled": false,
            "retention_days": 30,
            "updated_at": "0",
            "attached_agent": "claude_code"
        }"#;
        let settings: CueSettings = serde_json::from_str(legacy).expect("legacy config loads");
        assert_eq!(settings.attached_agent.as_deref(), Some("claude_code"));
        assert_eq!(settings.attached_session, None);
    }

    #[test]
    fn legacy_settings_without_attached_model_still_load() {
        // A config written after `attached_session` existed but before
        // `attached_model` was added must still deserialize, defaulting the
        // model to `None`.
        let legacy = r#"{
            "default_model": "Bluey Auto",
            "default_mode": "General",
            "answer_style": null,
            "overlay_opacity": 0.9,
            "audio_system_enabled": true,
            "audio_microphone_enabled": true,
            "cloud_sync_enabled": false,
            "retention_days": 30,
            "updated_at": "0",
            "attached_agent": "claude_code",
            "attached_session": "sess-42"
        }"#;
        let settings: CueSettings = serde_json::from_str(legacy).expect("legacy config loads");
        assert_eq!(settings.attached_agent.as_deref(), Some("claude_code"));
        assert_eq!(settings.attached_session.as_deref(), Some("sess-42"));
        assert_eq!(settings.attached_model, None);
        // Added after this legacy config was written; must default to false
        // (unprimed → the attached session sends full context on its first turn).
        assert!(!settings.attached_context_primed);
    }
}
