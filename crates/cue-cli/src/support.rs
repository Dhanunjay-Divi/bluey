//! `bluey support` — one-shot support bundle for tickets.
//!
//! Combines `bluey doctor --json` + `bluey logs export --redact` into a
//! single zip the customer attaches to a support ticket. Redacted by
//! default; `--no-redact` opt-out for cases where the customer is sharing
//! the bundle privately with a trusted engineer.
//!
//! Output zip layout:
//!
//!     Bluey-support-YYYYMMDD.zip
//!       manifest.json           — what's in the bundle + redaction state
//!       doctor.json             — bluey doctor --json output
//!       system-info.txt         — uname + sw_vers (macOS only)
//!       logs/
//!         daemon-log.YYYY-MM-DD.log  (redacted)
//!         dashboard-log.YYYY-MM-DD.log  (redacted)
//!         ...
//!
//! Phase 4 follow-up: the most-requested support-tooling feature.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

#[derive(Debug)]
pub struct SupportArgs {
    pub redact: bool,
    pub days: u32,
    pub output: Option<PathBuf>,
}

pub fn bundle(args: SupportArgs) -> Result<()> {
    let stamp = current_yyyymmdd();
    let suffix = if args.redact { "redacted" } else { "raw" };
    let output = args.output.unwrap_or_else(|| {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(format!("Bluey-support-{stamp}-{suffix}.zip"))
    });

    let zip_file =
        fs::File::create(&output).with_context(|| format!("create {}", output.display()))?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let entry_options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);

    // ── 1. doctor.json ──────────────────────────────────────────────
    let doctor_json = capture_doctor_json()?;
    zip.start_file("doctor.json", entry_options)
        .context("zip doctor.json")?;
    zip.write_all(doctor_json.as_bytes())
        .context("write doctor.json")?;

    // ── 2. system-info.txt ──────────────────────────────────────────
    let sysinfo = capture_system_info();
    zip.start_file("system-info.txt", entry_options)
        .context("zip system-info.txt")?;
    zip.write_all(sysinfo.as_bytes())
        .context("write system-info.txt")?;

    // ── 3. logs/ ────────────────────────────────────────────────────
    let log_dir = cue_core::local_log_dir();
    let mut log_files: Vec<(String, usize)> = Vec::new();
    if log_dir.exists() {
        let cutoff = age_cutoff(args.days);
        let mut found: Vec<_> = fs::read_dir(&log_dir)
            .with_context(|| format!("read_dir {}", log_dir.display()))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".log")))
            .collect();
        found.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

        for entry in found {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            if modified < cutoff {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let path = entry.path();
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let processed = if args.redact {
                crate::logs::redact_log_content(&raw)
            } else {
                raw
            };
            let entry_name = format!("logs/{name}");
            zip.start_file(&entry_name, entry_options)
                .with_context(|| format!("zip {entry_name}"))?;
            zip.write_all(processed.as_bytes())
                .with_context(|| format!("write {entry_name}"))?;
            log_files.push((name, processed.lines().count()));
        }
    }

    // ── 4. manifest.json ────────────────────────────────────────────
    let manifest = serde_json::json!({
        "schema_version": 1,
        "generated_at_unix_ms": current_unix_ms(),
        "redacted": args.redact,
        "redaction_policy": if args.redact {
            "tokens, magic-link URLs, Stripe IDs, OpenAI/Anthropic/Deepgram \
             keys, JWTs, device codes, emails, IPv4 addresses (last octet → 0/24), \
             /Users/<name>/ + /home/<name>/ paths"
        } else {
            "NONE — raw content; do not share publicly"
        },
        "preserved_for_correlation": [
            "trace_id", "request_id", "session_id", "account_id_hash",
        ],
        "contents": {
            "doctor.json": "bluey doctor --json output",
            "system-info.txt": "uname + sw_vers + arch",
            "logs/*": log_files.iter().map(|(name, lines)| {
                serde_json::json!({"file": name, "lines": lines})
            }).collect::<Vec<_>>(),
        },
        "tool_version": env!("CARGO_PKG_VERSION"),
    });
    zip.start_file("manifest.json", entry_options)
        .context("zip manifest.json")?;
    zip.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())
        .context("write manifest.json")?;

    zip.finish().context("finalize support zip")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&output, fs::Permissions::from_mode(0o600));
    }

    let total_log_files = log_files.len();
    let total_log_lines: usize = log_files.iter().map(|(_, n)| n).sum();
    println!("Bluey support bundle written to {}", output.display());
    println!();
    println!("  doctor.json       : 1 file (always included)");
    println!("  system-info.txt   : 1 file (always included)");
    println!(
        "  logs/             : {} file{} ({} lines)",
        total_log_files,
        if total_log_files == 1 { "" } else { "s" },
        total_log_lines,
    );
    println!("  manifest.json     : 1 file describing contents + redaction");
    if !log_dir.exists() {
        println!();
        println!("  NOTE: no log directory at {}.", log_dir.display());
        println!("  Daemon log rotation may not be enabled yet.");
    }
    println!();
    if args.redact {
        println!("Bundle is REDACTED (default). Safe to attach to a public");
        println!("support ticket. Tokens, emails, paths to /Users/<name>/,");
        println!("provider keys, and IPv4 last-octets are masked.");
    } else {
        println!("WARNING: --no-redact applied. Bundle contains raw content");
        println!("which may include tokens, paths, or sensitive metadata.");
        println!("Share only over a private channel with a trusted engineer.");
    }
    Ok(())
}

/// Capture `bluey doctor --json` output as a string by running the same
/// probes inline. We don't shell out to ourselves — that would risk a
/// recursive process spawn. We just call the same code paths.
fn capture_doctor_json() -> Result<String> {
    // Re-route stdout into a Vec<u8> for the duration of run_json().
    // Rust doesn't make this trivial without unsafe; simpler is to
    // duplicate the JSON-build logic. Keep the canonical implementation
    // in doctor::run_json() — here we just call it indirectly via a
    // public helper.
    crate::doctor::collect_json_string()
}

fn capture_system_info() -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "os = {}\narch = {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    ));

    #[cfg(target_os = "macos")]
    {
        if let Ok(o) = std::process::Command::new("sw_vers").output() {
            if o.status.success() {
                out.push_str(&format!(
                    "\n--- sw_vers ---\n{}",
                    String::from_utf8_lossy(&o.stdout)
                ));
            }
        }
    }
    if let Ok(o) = std::process::Command::new("uname").arg("-a").output() {
        if o.status.success() {
            out.push_str(&format!(
                "\n--- uname -a ---\n{}",
                String::from_utf8_lossy(&o.stdout)
            ));
        }
    }
    out
}

fn age_cutoff(days: u32) -> std::time::SystemTime {
    let secs = u64::from(days).saturating_mul(86_400);
    std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(secs))
        .unwrap_or(std::time::UNIX_EPOCH)
}

fn current_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn current_yyyymmdd() -> String {
    crate::logs::current_yyyymmdd_for_support()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_system_info_returns_non_empty() {
        let s = capture_system_info();
        assert!(s.contains("os = "));
        assert!(s.contains("arch = "));
    }

    #[test]
    fn current_yyyymmdd_format() {
        let s = current_yyyymmdd();
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_ascii_digit()));
    }
}
