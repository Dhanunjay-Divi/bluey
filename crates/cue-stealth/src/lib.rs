//! Process masquerading for the Cue dashboard.
//!
//! Provides cross-platform process-name and window-identity spoofing so the
//! running application appears as a benign system utility (Terminal, Settings,
//! Activity Monitor) in task managers and window lists.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Disguise mode - which system app to impersonate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DisguiseMode {
    Terminal,
    Settings,
    Activity,
    #[default]
    None,
}

impl DisguiseMode {
    /// Parse from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "terminal" => Self::Terminal,
            "settings" => Self::Settings,
            "activity" => Self::Activity,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Settings => "settings",
            Self::Activity => "activity",
            Self::None => "none",
        }
    }
}

/// Per-mode metadata used to apply the disguise.
pub struct DisguiseRequest {
    pub mode: DisguiseMode,
    pub app_name: String,
    pub icon_path: Option<PathBuf>,
    /// Windows App User Model ID for taskbar grouping.
    pub aumid: Option<String>,
    /// When true, skip operations that re-register the app with the OS
    /// (e.g. macOS dock icon changes that cause the app to reappear).
    pub is_undetectable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum StealthError {
    #[error("platform not supported for this operation")]
    Unsupported,
    #[error("ffi error: {0}")]
    Ffi(String),
}

/// Default app name when disguise is disabled.
const DEFAULT_APP_NAME: &str = "Bluey";

/// Build a [`DisguiseRequest`] from a mode using platform-appropriate defaults.
///
/// `icon_base_dir` is the directory containing platform icon subdirectories
/// (e.g. `mac/`, `win/`, `linux/`). If `None`, icon_path will be `None`.
pub fn build_request(
    mode: DisguiseMode,
    icon_base_dir: Option<&std::path::Path>,
) -> DisguiseRequest {
    let (app_name, icon_file, aumid_suffix) = mode_metadata(mode);
    let icon_path = icon_base_dir.and_then(|base| {
        icon_file.map(|file| {
            let platform_dir = if cfg!(target_os = "macos") {
                "mac"
            } else if cfg!(target_os = "windows") {
                "win"
            } else {
                "linux"
            };
            base.join(platform_dir).join(file)
        })
    });
    let aumid = aumid_suffix.map(|s| format!("com.bluey.cue.{s}"));
    DisguiseRequest {
        mode,
        app_name: app_name.to_owned(),
        icon_path,
        aumid,
        is_undetectable: false,
    }
}

/// Returns (app_name, icon_filename, aumid_suffix) for a given mode.
pub fn mode_metadata(
    mode: DisguiseMode,
) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match mode {
        DisguiseMode::Terminal => {
            let name = if cfg!(target_os = "windows") {
                "Command Prompt "
            } else {
                "Terminal "
            };
            (name, Some("terminal.png"), Some("terminal"))
        }
        DisguiseMode::Settings => {
            let name = if cfg!(target_os = "windows") {
                "Settings "
            } else {
                "System Settings "
            };
            (name, Some("settings.png"), Some("settings"))
        }
        DisguiseMode::Activity => {
            let name = if cfg!(target_os = "windows") {
                "Task Manager "
            } else {
                "Activity Monitor "
            };
            (name, Some("activity.png"), Some("activity"))
        }
        DisguiseMode::None => (DEFAULT_APP_NAME, None, None),
    }
}

/// Apply process-level masquerading for the current platform.
///
/// This modifies the process title / argv[0] so that system tools (Activity
/// Monitor, Task Manager, `ps`) show the disguised name. Window-level changes
/// (title, icon) must be done separately via the Tauri window API.
pub fn apply_disguise(req: &DisguiseRequest) -> Result<(), StealthError> {
    tracing::info!(mode = ?req.mode, name = %req.app_name, "applying process disguise");

    // Set CFBundleName on macOS (best-effort for menu bar)
    #[cfg(target_os = "macos")]
    {
        // SAFETY: single-threaded at startup; env var is read by AppKit.
        unsafe { std::env::set_var("CFBundleName", req.app_name.trim()) };
    }

    #[cfg(target_os = "macos")]
    macos::set_process_name(&req.app_name)?;

    #[cfg(target_os = "linux")]
    linux::set_process_name(&req.app_name)?;

    #[cfg(target_os = "windows")]
    if let Some(aumid) = &req.aumid {
        windows::set_app_user_model_id(aumid)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_str_round_trip() {
        for mode in [
            DisguiseMode::Terminal,
            DisguiseMode::Settings,
            DisguiseMode::Activity,
            DisguiseMode::None,
        ] {
            assert_eq!(DisguiseMode::from_str_loose(mode.as_str()), mode);
        }
    }

    #[test]
    fn mode_metadata_terminal() {
        let (name, icon, aumid) = mode_metadata(DisguiseMode::Terminal);
        if cfg!(target_os = "windows") {
            assert_eq!(name, "Command Prompt ");
        } else {
            assert_eq!(name, "Terminal ");
        }
        assert_eq!(icon, Some("terminal.png"));
        assert_eq!(aumid, Some("terminal"));
    }

    #[test]
    fn mode_metadata_settings() {
        let (name, icon, aumid) = mode_metadata(DisguiseMode::Settings);
        if cfg!(target_os = "windows") {
            assert_eq!(name, "Settings ");
        } else {
            assert_eq!(name, "System Settings ");
        }
        assert_eq!(icon, Some("settings.png"));
        assert_eq!(aumid, Some("settings"));
    }

    #[test]
    fn mode_metadata_activity() {
        let (name, icon, aumid) = mode_metadata(DisguiseMode::Activity);
        if cfg!(target_os = "windows") {
            assert_eq!(name, "Task Manager ");
        } else {
            assert_eq!(name, "Activity Monitor ");
        }
        assert_eq!(icon, Some("activity.png"));
        assert_eq!(aumid, Some("activity"));
    }

    #[test]
    fn mode_metadata_none() {
        let (name, icon, aumid) = mode_metadata(DisguiseMode::None);
        assert_eq!(name, DEFAULT_APP_NAME);
        assert!(icon.is_none());
        assert!(aumid.is_none());
    }

    #[test]
    fn build_request_without_icon_dir() {
        let req = build_request(DisguiseMode::Terminal, None);
        assert!(req.icon_path.is_none());
        assert!(req.aumid.is_some());
    }

    #[test]
    fn build_request_with_icon_dir() {
        let base = std::path::Path::new("/fake/icons");
        let req = build_request(DisguiseMode::Settings, Some(base));
        assert!(req.icon_path.is_some());
        let p = req.icon_path.unwrap();
        assert!(p.to_str().unwrap().contains("settings.png"));
    }

    #[test]
    fn apply_disguise_none_does_not_panic() {
        let req = build_request(DisguiseMode::None, None);
        let result = apply_disguise(&req);
        assert!(result.is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apply_disguise_terminal_macos() {
        let req = build_request(DisguiseMode::Terminal, None);
        let result = apply_disguise(&req);
        assert!(result.is_ok());
        // Verify CFBundleName was set
        assert_eq!(std::env::var("CFBundleName").unwrap(), "Terminal");
    }
}
