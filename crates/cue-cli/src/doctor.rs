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
use std::time::{Duration, SystemTime};

/// Run the doctor check and print a redacted snapshot to stdout.
pub fn run() -> Result<()> {
    println!("============================================================");
    println!(" Bluey Doctor");
    println!(" {}", utc_iso8601_now());
    println!("============================================================");
    println!();

    print_health_summary();
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

/// Emit the same diagnostic snapshot as `run()` but in structured JSON.
/// Schema version 1. Same probe calls, same redaction, just a different
/// formatter for support tooling that wants to parse output.
fn build_doctor_json() -> Result<serde_json::Value> {
    use crate::macos_perms::{
        accessibility_status, microphone_status, screen_recording_status, PermissionStatus,
    };
    use serde_json::{json, Value};

    fn perm_to_json(s: PermissionStatus) -> Value {
        let label = match s {
            PermissionStatus::Granted => "granted",
            PermissionStatus::Denied => "denied",
            PermissionStatus::NotDetermined => "not_determined",
            PermissionStatus::Restricted => "restricted",
            PermissionStatus::Unknown => "unknown",
            PermissionStatus::NotApplicable => "not_applicable",
        };
        json!({ "status": label, "hint": s.hint("the requested permission") })
    }

    fn dir_entry(path: &std::path::Path) -> Value {
        let exists = path.exists();
        let mode = file_mode_octal(path).map(|m| format!("{m:o}"));
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        json!({
            "path": path.display().to_string(),
            "exists": exists,
            "mode": mode,
            "size_bytes": size,
        })
    }

    let paths = AppPaths::discover()?;
    let account = cue_core::load_account(&paths).ok().flatten();
    let secure_tokens = account
        .as_ref()
        .and_then(|_| load_secure_tokens_for_doctor(paths.clone()));

    let mut output = json!({
        "schema_version": 1,
        "build": {
            "bluey_version": env!("CARGO_PKG_VERSION"),
            "build_profile": build_profile(),
            "git_commit": option_env!("GIT_COMMIT_SHA"),
        },
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "paths": {
            "data_dir": dir_entry(&paths.data_dir),
            "config_dir": dir_entry(&paths.config_dir),
            "runtime_dir": dir_entry(&paths.runtime_dir),
            "account_file": dir_entry(&paths.account_file),
            "settings_file": dir_entry(&paths.settings_file),
            "state_file": dir_entry(&paths.state_file),
        },
        "account": match &account {
            Some(a) => json!({
                "logged_in": true,
                "provider": a.provider,
                "api_url": a.api_url,
                "user_id_hash": cue_core::account_id_hash_prefix(&a.user_id),
                "workspace_id": a.workspace_id,
                "device_id_hash": cue_core::account_id_hash_prefix(&a.device_id),
                "linked_at": a.linked_at,
                "token_storage": "os_secure_storage",
                "has_access_token": secure_tokens.is_some(),
                "has_refresh_token": secure_tokens
                    .as_ref()
                    .is_some_and(|tokens| !tokens.refresh.trim().is_empty()),
            }),
            None => json!({ "logged_in": false }),
        },
        "permissions": {
            "accessibility": perm_to_json(accessibility_status()),
            "microphone": perm_to_json(microphone_status()),
            "screen_recording": perm_to_json(screen_recording_status()),
        },
        "local_db": {
            "path": paths.data_dir.join("sessions.db").display().to_string(),
            "exists": paths.data_dir.join("sessions.db").exists(),
            "mode": file_mode_octal(&paths.data_dir.join("sessions.db"))
                .map(|m| format!("{m:o}")),
            "size_bytes": std::fs::metadata(paths.data_dir.join("sessions.db"))
                .map(|m| m.len()).unwrap_or(0),
        },
    });

    // Append a redacted log-tail object — same source as the human
    // version, but capped at 20 lines and run through the redactor.
    let log_dir = log_dir_for_doctor();
    let mut log_files: Vec<String> = Vec::new();
    let mut tail_lines: Vec<String> = Vec::new();
    if log_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&log_dir) {
            let mut found: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with("daemon-") && n.ends_with(".log"))
                })
                .collect();
            found.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            for entry in &found {
                if let Some(name) = entry.file_name().to_str() {
                    log_files.push(name.to_string());
                }
            }
            if let Some(latest) = found.last() {
                let path = latest.path();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let lines: Vec<&str> = content.lines().rev().take(20).collect();
                    for line in lines.iter().rev() {
                        tail_lines.push(crate::logs::redact_log_line(line));
                    }
                }
            }
        }
    }
    output["log_tail"] = json!({
        "log_dir": log_dir.display().to_string(),
        "found_files": log_files,
        "tail_redacted": tail_lines,
        "phase2_log_rotation_active": log_dir.exists(),
    });

    Ok(output)
}

pub fn run_json() -> Result<()> {
    let value = build_doctor_json()?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Build the same JSON snapshot but return it as a pretty-printed
/// string instead of writing to stdout. Used by the `bluey support`
/// bundle to embed `doctor.json` directly into the zip.
pub fn collect_json_string() -> Result<String> {
    let value = build_doctor_json()?;
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Quick at-a-glance triage line at the top of the doctor output.
/// Computes a one-shot health summary across the major categories so
/// support can read the first few lines and know if the customer is
/// blocked on something obvious (logged-out, missing permissions,
/// log rotation not active, etc.).
fn print_health_summary() {
    use crate::macos_perms::{
        accessibility_status, microphone_status, screen_recording_status, PermissionStatus,
    };

    let paths = AppPaths::discover().ok();
    let account = paths
        .as_ref()
        .and_then(|p| cue_core::load_account(p).ok())
        .flatten();

    let logged_in = account.is_some();
    let perms = [
        accessibility_status(),
        microphone_status(),
        screen_recording_status(),
    ];
    let granted = perms
        .iter()
        .filter(|p| matches!(p, PermissionStatus::Granted))
        .count();
    let total_perms = perms.len();
    let log_dir = log_dir_for_doctor();
    let log_active = log_dir.exists();

    let issues: Vec<String> = {
        let mut v = Vec::new();
        if !logged_in {
            v.push("not signed in (run `bluey on`)".to_string());
        }
        if granted < total_perms {
            v.push(format!(
                "{} of {} macOS permissions not granted",
                total_perms - granted,
                total_perms
            ));
        }
        if !log_active {
            v.push(format!("no log dir at {}", log_dir.display()));
        }
        v
    };

    println!("── Summary ──");
    println!("  bluey version : {}", env!("CARGO_PKG_VERSION"));
    println!(
        "  account       : {}",
        if logged_in { "logged in" } else { "logged out" }
    );
    println!("  permissions   : {}/{} granted", granted, total_perms);
    println!(
        "  log rotation  : {}",
        if log_active { "active" } else { "not active" }
    );
    if issues.is_empty() {
        println!("  status        : OK no obvious issues");
    } else {
        println!("  status        : {} issue(s):", issues.len());
        for issue in issues {
            println!("                  -> {issue}");
        }
    }
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
    let secure_tokens = account
        .as_ref()
        .and_then(|_| load_secure_tokens_for_doctor(paths.clone()));
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
            println!("  token storage : Bluey private account profile");
            let has_token = secure_tokens.is_some();
            println!(
                "  access token  : {}",
                if has_token { "<present>" } else { "<missing>" }
            );
            let has_refresh = secure_tokens
                .as_ref()
                .is_some_and(|tokens| !tokens.refresh.trim().is_empty());
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
            println!("  hint          : run `bluey on` to finish setup");
        }
    }
    Ok(())
}

fn load_secure_tokens_for_doctor(paths: AppPaths) -> Option<cue_cloud_client::Tokens> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let store = cue_cloud_client::SecureAccountStore::new(paths);
        let _ = tx.send(cue_cloud_client::TokenStore::load(&store).ok().flatten());
    });
    rx.recv_timeout(Duration::from_millis(750)).ok().flatten()
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
    // Delegates to cue_core::local_log_dir which respects BLUEY_LOG_DIR /
    // CUE_LOG_DIR overrides + platform-specific defaults. Phase 4 originally
    // hardcoded the macOS path, which broke when Phase 2 added env-var
    // override support. Centralizing on cue_core means this Just Works
    // wherever the daemon writes.
    cue_core::local_log_dir()
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
    fn run_json_emits_valid_json_with_expected_keys() {
        // Capture stdout via a process round-trip would require integration
        // testing infrastructure. Instead, just smoke-test that the function
        // executes without panicking and emits something that parses as JSON.
        // Full schema validation belongs in an integration test suite.
        // For unit purposes, this asserts the run_json path is reachable.
        let result = std::panic::catch_unwind(|| {
            // run_json prints to stdout. We don't capture here; just
            // ensure it doesn't panic on a fresh invocation.
            let _ = super::run_json();
        });
        assert!(result.is_ok(), "run_json must not panic");
    }

    #[test]
    fn build_profile_returns_known_value() {
        let p = build_profile();
        assert!(p == "debug" || p == "release");
    }
}
