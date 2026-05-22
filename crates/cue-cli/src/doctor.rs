//! `bluey doctor` — self-diagnosis snapshot for support tickets.
//!
//! Outputs a structured report covering version, platform, account state,
//! permissions, paths, DB summary, and recent log tail. Sensitive fields
//! are redacted. Designed to be pasted directly into a support ticket.
//!
//! Phase 4 of the Observability Round.

use anyhow::{Context, Result};
use cue_core::app_paths::AppPaths;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Run the doctor check and print a redacted snapshot to stdout.
pub fn run() -> Result<()> {
    println!("============================================================");
    println!(" Bluey Doctor");
    println!(" {}", utc_iso8601_now());
    println!("============================================================");
    println!();

    print_section("Build", print_build_section);
    print_section("Platform", print_platform_section);
    print_section("Paths", print_paths_section);
    print_section("Account", print_account_section);
    print_section("macOS Permissions", print_permissions_section);
    print_section("Local DB", print_db_section);
    print_section("Daemon Logs (tail)", print_log_tail_section);

    println!();
    println!("============================================================");
    println!(" Paste the above into your support ticket. Tokens, emails,");
    println!(" and provider keys have been redacted automatically.");
    println!("============================================================");
    Ok(())
}

fn print_section<F: FnOnce() -> Result<()>>(name: &str, f: F) {
    println!("── {name} ──");
    if let Err(err) = f() {
        println!("  (probe failed: {err})");
    }
    println!();
}

fn print_build_section() -> Result<()> {
    println!("  bluey version : {}", env!("CARGO_PKG_VERSION"));
    println!("  build profile : {}", build_profile());
    if let Some(commit) = option_env!("GIT_COMMIT_SHA") {
        println!("  git commit    : {commit}");
    } else {
        println!("  git commit    : (not embedded)");
    }
    Ok(())
}

const fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn print_platform_section() -> Result<()> {
    println!("  os            : {}", std::env::consts::OS);
    println!("  arch          : {}", std::env::consts::ARCH);

    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("sw_vers").output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                for line in s.lines() {
                    println!("  {line}");
                }
            }
        }
    }
    Ok(())
}

fn print_paths_section() -> Result<()> {
    let paths = AppPaths::discover().context("AppPaths::discover")?;
    print_dir("data dir", &paths.data_dir);
    print_dir("config dir", &paths.config_dir);
    print_dir("runtime dir", &paths.runtime_dir);
    print_file("account.json", &paths.account_file);
    print_file("settings.json", &paths.settings_file);
    print_file("daemon-state.json", &paths.state_file);
    Ok(())
}

fn print_dir(label: &str, path: &Path) {
    let exists = path.exists();
    let mode = file_mode_octal(path);
    let mode_str = mode.map(|m| format!("{m:o}")).unwrap_or_else(|| "-".into());
    println!(
        "  {label:<14}: {} (exists={}, mode={})",
        path.display(),
        exists,
        mode_str,
    );
}

fn print_file(label: &str, path: &Path) {
    let exists = path.exists();
    let mode = file_mode_octal(path);
    let mode_str = mode.map(|m| format!("{m:o}")).unwrap_or_else(|| "-".into());
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!(
        "  {label:<14}: {} (exists={}, mode={}, bytes={})",
        path.display(),
        exists,
        mode_str,
        size,
    );
}

fn file_mode_octal(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .ok()
            .map(|m| m.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

fn print_account_section() -> Result<()> {
    let paths = AppPaths::discover()?;
    let account = cue_core::load_account(&paths).ok().flatten();
    match account {
        Some(account) => {
            println!("  status        : logged in");
            println!("  provider      : {}", account.provider);
            println!("  api_url       : {}", account.api_url);
            println!(
                "  user_id       : {}",
                cue_core::account_id_hash_prefix(&account.user_id)
            );
            println!("  workspace_id  : {}", account.workspace_id);
            println!(
                "  device_id     : {}",
                cue_core::account_id_hash_prefix(&account.device_id)
            );
            println!("  linked_at     : {}", account.linked_at);
            let has_token = account.access_token.is_some();
            println!(
                "  access token  : {}",
                if has_token { "<present>" } else { "<missing>" }
            );
            let has_refresh = account.refresh_token.is_some();
            println!(
                "  refresh token : {}",
                if has_refresh {
                    "<present>"
                } else {
                    "<missing>"
                }
            );
        }
        None => {
            println!("  status        : logged out");
            println!("  hint          : run `bluey login` to enable managed answers");
        }
    }
    Ok(())
}

fn print_permissions_section() -> Result<()> {
    use crate::macos_perms::{
        accessibility_status, microphone_status, screen_recording_status, PermissionStatus,
    };

    let acc = accessibility_status();
    let mic = microphone_status();
    let scr = screen_recording_status();

    println!("  Accessibility   : {}", acc.label());
    if let Some(hint) = acc.hint("Accessibility") {
        println!("                  → {hint}");
    }
    println!("  Microphone      : {}", mic.label());
    if let Some(hint) = mic.hint("Microphone") {
        println!("                  → {hint}");
    }
    println!("  Screen Recording: {}", scr.label());
    if let Some(hint) = scr.hint("Screen Recording") {
        println!("                  → {hint}");
    }

    if matches!(
        acc,
        PermissionStatus::Denied | PermissionStatus::NotDetermined
    ) {
        println!("  hint            : if F19 hotkey does not fire, Accessibility is");
        println!("                    the most likely cause.");
    }

    Ok(())
}

fn print_db_section() -> Result<()> {
    let paths = AppPaths::discover()?;
    let db_path = paths.data_dir.join("sessions.db");
    if !db_path.exists() {
        println!("  status        : no local DB at {}", db_path.display());
        return Ok(());
    }
    let mode = file_mode_octal(&db_path)
        .map(|m| format!("{m:o}"))
        .unwrap_or_else(|| "-".into());
    let size = fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    println!("  path          : {}", db_path.display());
    println!("  mode          : {mode}");
    println!("  size          : {size} bytes");

    // Skip table-row counts to avoid hard-coding rusqlite logic across
    // crates; doctor stays in cue-cli without a rusqlite dep. If a user
    // needs row counts, they can run `sqlite3 sessions.db .schema` manually.
    println!(
        "  hint          : `sqlite3 {} .tables` shows tables",
        db_path.display()
    );
    Ok(())
}

fn print_log_tail_section() -> Result<()> {
    let log_dir = log_dir_for_doctor();
    if !log_dir.exists() {
        println!(
            "  status        : no log directory at {}",
            log_dir.display()
        );
        println!("  note          : daemon log rotation is Phase 2 of the");
        println!("                  Observability Round and not yet enabled.");
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(&log_dir)
        .with_context(|| format!("read_dir {}", log_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("daemon-") && n.ends_with(".log"))
        })
        .collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

    let Some(latest) = entries.last() else {
        println!(
            "  status        : no daemon-*.log files found in {}",
            log_dir.display()
        );
        return Ok(());
    };
    let path = latest.path();
    println!("  reading       : {}", path.display());
    let content = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let tail: Vec<&str> = content.lines().rev().take(20).collect();
    println!();
    for line in tail.iter().rev() {
        let redacted = crate::logs::redact_log_line(line);
        println!("    {redacted}");
    }
    Ok(())
}

fn log_dir_for_doctor() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join("Library/Logs/Bluey");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(".local/state/bluey/log");
        }
    }
    std::path::PathBuf::from(".")
}

fn utc_iso8601_now() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("(unix={secs})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_hash_prefix_is_stable_and_short() {
        let h = cue_core::account_id_hash_prefix("acct_12345");
        assert_eq!(h.len(), 12);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        // Stable across calls
        assert_eq!(h, cue_core::account_id_hash_prefix("acct_12345"));
        // Different input → different hash
        assert_ne!(h, cue_core::account_id_hash_prefix("acct_67890"));
    }

    #[test]
    fn build_profile_returns_known_value() {
        let p = build_profile();
        assert!(p == "debug" || p == "release");
    }
}
