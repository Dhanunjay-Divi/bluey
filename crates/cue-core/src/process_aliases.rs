//! Stable executable names shared by Bluey's CLI and native runtime.
//!
//! The first name in each list is the preferred Bluey-owned process identity.
//! Remaining names are compatibility fallbacks for existing installations.

use std::path::Path;

pub const DAEMON_EXECUTABLE_STEMS: &[&str] = &["termb", "Terminal", "bluey-daemon", "cue-daemon"];

pub const MACOS_AUDIO_HELPER_NAMES: &[&str] = &[
    "adriverb",
    "audio-driver",
    "bluey-audio-macos",
    "cue-audio-macos",
];

pub const WINDOWS_AUDIO_HELPER_NAMES: &[&str] = &[
    "adriverb.exe",
    "audio-driver.exe",
    "bluey-audio.exe",
    "cue-audio.exe",
];

pub const MACOS_HOST_OVERLAY_BINARY_NAMES: &[&str] = &["hostovb", "host-overlay"];

pub const MACOS_OVERLAY_BINARY_NAMES: &[&str] = &[
    "hostovb",
    "host-overlay",
    "bluey-overlay-macos",
    "cue-overlay-macos",
];

pub const MACOS_OVERLAY_APP_BUNDLE_NAMES: &[&str] = &[
    "BlueyOverlay.app",
    "hostovb.app",
    "host-overlay.app",
    "bluey-overlay-macos.app",
    "cue-overlay-macos.app",
];

pub const WINDOWS_OVERLAY_BINARY_NAMES: &[&str] = &[
    "hostovb.exe",
    "host-overlay.exe",
    "bluey-overlay.exe",
    "cue-overlay.exe",
];

pub fn normalized_executable_stem(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    Some(name.strip_suffix(".exe").unwrap_or(&name).to_string())
}

pub fn is_daemon_identity_path(path: &Path) -> bool {
    matches!(
        normalized_executable_stem(path).as_deref(),
        Some("termb" | "terminal")
    )
}

pub fn is_daemon_executable_path(path: &Path) -> bool {
    matches!(
        normalized_executable_stem(path).as_deref(),
        Some("termb" | "terminal" | "bluey-daemon" | "cue-daemon")
    )
}

pub fn executable_name_is_one_of(path: &Path, names: &[&str]) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            names
                .iter()
                .any(|candidate| name.eq_ignore_ascii_case(candidate))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_aliases_are_first() {
        assert_eq!(DAEMON_EXECUTABLE_STEMS.first(), Some(&"termb"));
        assert_eq!(MACOS_AUDIO_HELPER_NAMES.first(), Some(&"adriverb"));
        assert_eq!(WINDOWS_AUDIO_HELPER_NAMES.first(), Some(&"adriverb.exe"));
        assert_eq!(MACOS_OVERLAY_BINARY_NAMES.first(), Some(&"hostovb"));
        assert_eq!(WINDOWS_OVERLAY_BINARY_NAMES.first(), Some(&"hostovb.exe"));
    }

    #[test]
    fn daemon_names_are_normalized_across_platforms() {
        assert!(is_daemon_identity_path(Path::new("/tmp/termb")));
        assert!(is_daemon_identity_path(Path::new("Terminal.exe")));
        assert!(is_daemon_executable_path(Path::new("bluey-daemon")));
        assert!(is_daemon_executable_path(Path::new("CUE-DAEMON.EXE")));
        assert!(!is_daemon_executable_path(Path::new("Terminal.app")));
    }

    #[test]
    fn helper_matching_is_case_insensitive() {
        assert!(executable_name_is_one_of(
            Path::new("HOSTOVB.EXE"),
            WINDOWS_OVERLAY_BINARY_NAMES
        ));
        assert!(!executable_name_is_one_of(
            Path::new("hostovb-copy.exe"),
            WINDOWS_OVERLAY_BINARY_NAMES
        ));
    }
}
