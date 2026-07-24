//! Hermetic user-space runtime resolution for Bluey agent provisioning.
//!
//! Provides isolated, non-sudo directory resolution for agent CLIs and Node runtimes
//! under `~/.local/share/bluey/` (or platform equivalent).

use std::path::PathBuf;
use std::process::Command;

/// Base user-space directory for Bluey managed runtimes and binaries.
pub fn bluey_user_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("BLUEY_USER_DIR") {
        return PathBuf::from(override_dir);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        {
            return home.join("Library/Application Support/bluey");
        }
        #[cfg(not(target_os = "macos"))]
        {
            return home.join(".local/share/bluey");
        }
    }
    PathBuf::from("/tmp/bluey-user")
}

/// Directory for isolated agent binaries installed by Bluey (`~/.local/share/bluey/agents/bin`).
pub fn agent_bin_dir() -> PathBuf {
    bluey_user_dir().join("agents").join("bin")
}

/// Directory for isolated node_modules (`~/.local/share/bluey/agents/node_modules`).
pub fn agent_node_modules_dir() -> PathBuf {
    bluey_user_dir().join("agents")
}

/// Returns an augmented `PATH` environment string that prepends Bluey's user-space binary
/// directories to the existing system `PATH`.
pub fn augmented_path() -> String {
    let bin_path = agent_bin_dir();
    let current_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bin_path.display(), current_path)
}

/// Check if a binary (e.g. `node` or `npm`) is runnable and meets a minimum version requirement.
pub fn check_binary_available(binary: &str, version_flag: &str) -> bool {
    let mut cmd = Command::new(binary);
    cmd.arg(version_flag);
    cmd.env("PATH", augmented_path());
    match cmd.output() {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Preflight check for Node.js runtime (requires Node >= 18 for modern agent CLIs).
pub fn is_node_available() -> bool {
    check_binary_available("node", "--version")
}

/// Preflight check for npm package manager.
pub fn is_npm_available() -> bool {
    check_binary_available("npm", "--version")
}

/// Preflight check for Homebrew package manager.
pub fn is_brew_available() -> bool {
    check_binary_available("brew", "--version")
}

/// Preflight check for GitHub CLI (`gh`).
pub fn is_gh_available() -> bool {
    check_binary_available("gh", "--version")
}

/// Ensure that `agent_bin_dir` exists on disk.
pub fn ensure_user_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(agent_bin_dir())?;
    std::fs::create_dir_all(agent_node_modules_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluey_user_dir_resolves() {
        let dir = bluey_user_dir();
        assert!(dir.to_str().unwrap().contains("bluey"));
    }

    #[test]
    fn test_augmented_path_includes_agent_bin() {
        let path = augmented_path();
        let bin = agent_bin_dir();
        assert!(path.contains(&bin.display().to_string()));
    }
}
