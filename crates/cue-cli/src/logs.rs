//! `bluey logs export --redact` — bundle and redact local Bluey logs.
//!
//! Phase 4 of the Observability Round.
//!
//! Finds daemon/dashboard log files (when log rotation lands in Phase 2),
//! pipes them through a regex-based redactor that strips bearer tokens,
//! magic-link URLs, Stripe IDs, provider keys, emails, and full IP
//! addresses, then bundles the result into a ZIP archive that a customer
//! can attach to a support ticket without leaking secrets.
//!
//! The redactor is reusable from `doctor.rs` for the log-tail section.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

#[derive(Debug)]
pub struct LogsExportArgs {
    pub redact: bool,
    pub days: u32,
    pub output: Option<PathBuf>,
}

pub fn export(args: LogsExportArgs) -> Result<()> {
    let log_dir = log_dir();
    if !log_dir.exists() {
        println!("No log directory at {}.", log_dir.display());
        println!("Daemon log rotation is Phase 2 of the Observability Round");
        println!("and not yet enabled. There is nothing to export today.");
        return Ok(());
    }

    let cutoff = age_cutoff(args.days);
    let log_files = find_log_files(&log_dir, cutoff)?;
    if log_files.is_empty() {
        println!(
            "No log files in {} within the last {} days.",
            log_dir.display(),
            args.days
        );
        return Ok(());
    }

    let output = args
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(args.redact));
    let zip_file =
        fs::File::create(&output).with_context(|| format!("create {}", output.display()))?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let zip_options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);

    let mut total_lines = 0usize;
    for log_file in &log_files {
        let name = log_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("log.txt");
        let raw =
            fs::read_to_string(log_file).with_context(|| format!("read {}", log_file.display()))?;
        let processed = if args.redact {
            redact_log_content(&raw)
        } else {
            raw
        };
        total_lines += processed.lines().count();
        zip.start_file(name, zip_options)
            .with_context(|| format!("zip add {name}"))?;
        zip.write_all(processed.as_bytes())
            .with_context(|| format!("zip write {name}"))?;
    }

    zip.finish().context("finalize zip")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&output, fs::Permissions::from_mode(0o600));
    }

    println!(
        "Exported {} file{} ({} lines) to {}",
        log_files.len(),
        if log_files.len() == 1 { "" } else { "s" },
        total_lines,
        output.display(),
    );
    if args.redact {
        println!();
        println!("Redacted: bearer tokens, magic-link URLs, Stripe IDs,");
        println!("OpenAI/Anthropic/Deepgram keys, email addresses, and full");
        println!("IP addresses (replaced with /24 prefix). Account IDs and");
        println!("session IDs are PRESERVED so support can correlate.");
    } else {
        println!();
        println!("WARNING: --redact NOT applied. The export contains raw log");
        println!("content which may include tokens or sensitive metadata.");
    }
    Ok(())
}

fn log_dir() -> PathBuf {
    // Delegates to cue_core::local_log_dir which respects BLUEY_LOG_DIR /
    // CUE_LOG_DIR overrides + platform-specific defaults. Centralized here
    // so Phase 2 env-var support is automatically honored by logs export.
    cue_core::local_log_dir()
}

fn age_cutoff(days: u32) -> std::time::SystemTime {
    let secs = u64::from(days).saturating_mul(86_400);
    std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(secs))
        .unwrap_or(std::time::UNIX_EPOCH)
}

fn find_log_files(log_dir: &Path, cutoff: std::time::SystemTime) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(log_dir).with_context(|| format!("read_dir {}", log_dir.display()))? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".log") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if modified < cutoff {
            continue;
        }
        out.push(path);
    }
    out.sort();
    Ok(out)
}

fn default_output_path(redact: bool) -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let suffix = if redact { "redacted" } else { "raw" };
    let stamp = current_yyyymmdd();
    home.join(format!("Bluey-logs-{stamp}-{suffix}.zip"))
}

fn current_yyyymmdd() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Days since epoch 1970-01-01 (UTC).
    let days = secs / 86_400;
    // Civil-from-days conversion (Howard Hinnant).
    let z = days as i64 + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let final_year = if m <= 2 { y + 1 } else { y };
    format!("{final_year:04}{m:02}{d:02}")
}

/// Redact a single log line. Public for `bluey doctor` to share the
/// redactor on its log-tail output.
pub fn redact_log_line(line: &str) -> String {
    redact_log_content(line)
}

/// Redact a multi-line log body. Returns the redacted text.
pub fn redact_log_content(content: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;

    static BEARER: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)bearer\s+[A-Za-z0-9._\-]+").unwrap());
    static MAGIC_LINK: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"bluey://[A-Za-z0-9?&=._\-/%+]+").unwrap());
    static STRIPE_IDS: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\b(?:cus|pi|seti|sub|in|pm|cs|sk|rk|whsec|price|prod|evt)_(?:test_|live_)?[A-Za-z0-9]+").unwrap()
    });
    static OPENAI: Lazy<Regex> = Lazy::new(|| Regex::new(r"\bsk-[A-Za-z0-9_\-]{20,}\b").unwrap());
    static ANTHROPIC: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\bsk-ant-[A-Za-z0-9_\-]{20,}\b").unwrap());
    static DEEPGRAM: Lazy<Regex> =
        // Deepgram keys are 40-char hex-ish; use a conservative pattern.
        Lazy::new(|| Regex::new(r"\b[a-f0-9]{40}\b").unwrap());
    static EMAIL: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap());
    static IPV4: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\b(\d{1,3}\.\d{1,3}\.\d{1,3})\.\d{1,3}\b").unwrap());
    // Mask the macOS-style /Users/<name>/ home prefix and the Linux-style
    // /home/<name>/ home prefix. The username is PII-adjacent and gets
    // stamped into log lines whenever a path reference is emitted (e.g.
    // Phase 2's `local log rotation initialized` message includes
    // `log_dir = /Users/<name>/Library/Logs/Bluey/`). Preserve the rest
    // of the path so support can still see what subtree the log refers
    // to (e.g. Library/Logs/Bluey).
    static USERS_HOME: Lazy<Regex> = Lazy::new(|| Regex::new(r"/Users/[^/]+/").unwrap());
    static LINUX_HOME: Lazy<Regex> = Lazy::new(|| Regex::new(r"/home/[^/]+/").unwrap());
    static JWT: Lazy<Regex> = Lazy::new(|| {
        // JWT-shaped: three base64url segments separated by '.'.
        Regex::new(r"\beyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\b").unwrap()
    });
    static DEVICE_CODE: Lazy<Regex> = Lazy::new(|| {
        // Loose device-code pattern — codes are usually 6-8 chars,
        // alphanumeric, often appearing after "code=" or "device_code".
        Regex::new(r#"(?i)(?:device[_-]?code|verification[_-]?code|verify[_-]?code|code)["':\s=]+["']?([A-Za-z0-9]{4,16})["']?"#).unwrap()
    });

    let mut out = content.to_string();

    // Order matters: replace longer-match patterns first.
    out = JWT.replace_all(&out, "<jwt>").into_owned();
    out = BEARER.replace_all(&out, "Bearer <token>").into_owned();
    out = MAGIC_LINK
        .replace_all(&out, "bluey://<redacted>")
        .into_owned();
    out = STRIPE_IDS.replace_all(&out, "<stripe_id>").into_owned();
    // ANTHROPIC must run before OPENAI: "sk-ant-..." matches the broader
    // OpenAI pattern "sk-[A-Za-z0-9_-]+" too, so the more-specific prefix
    // gets first claim.
    out = ANTHROPIC.replace_all(&out, "<anthropic_key>").into_owned();
    out = OPENAI.replace_all(&out, "<openai_key>").into_owned();
    // Apply Deepgram AFTER the others so we don't catch JWT/key fragments.
    out = DEEPGRAM.replace_all(&out, "<provider_key>").into_owned();
    out = EMAIL.replace_all(&out, "<email>").into_owned();
    out = IPV4.replace_all(&out, "$1.0/24").into_owned();
    out = USERS_HOME
        .replace_all(&out, "/Users/<redacted>/")
        .into_owned();
    out = LINUX_HOME
        .replace_all(&out, "/home/<redacted>/")
        .into_owned();
    out = DEVICE_CODE
        .replace_all(&out, "code=<redacted>")
        .into_owned();

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_strips_bearer_token() {
        let line = "GET /api Authorization: Bearer abc.def-123_xyz status=200";
        let r = redact_log_line(line);
        assert!(!r.contains("abc.def-123_xyz"));
        assert!(r.contains("Bearer <token>"));
    }

    #[test]
    fn redact_strips_magic_link() {
        let line = "deep link received: bluey://link?code=ABC12345&tenant=foo";
        let r = redact_log_line(line);
        assert!(!r.contains("ABC12345"));
        assert!(r.contains("bluey://<redacted>"));
    }

    #[test]
    fn redact_strips_stripe_ids() {
        let line = "stripe checkout cus_abc123 → cs_test_xyz789 priced price_def";
        let r = redact_log_line(line);
        assert!(!r.contains("cus_abc123"));
        assert!(!r.contains("cs_test_xyz789"));
        assert!(!r.contains("price_def"));
    }

    #[test]
    fn redact_strips_openai_key() {
        let line = "OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz1234567890 used";
        let r = redact_log_line(line);
        assert!(!r.contains("sk-proj-"));
        assert!(r.contains("<openai_key>"));
    }

    #[test]
    fn redact_strips_anthropic_key() {
        let line = "claude api sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789-XYZ used";
        let r = redact_log_line(line);
        assert!(!r.contains("sk-ant-api03-"));
        assert!(r.contains("<anthropic_key>"));
    }

    #[test]
    fn redact_strips_email() {
        let line = "verify alice@example.com sent at 12:00";
        let r = redact_log_line(line);
        assert!(!r.contains("alice@example.com"));
        assert!(r.contains("<email>"));
    }

    #[test]
    fn redact_masks_ipv4_to_slash24() {
        let line = "request from 192.168.4.25 took 12ms";
        let r = redact_log_line(line);
        assert!(!r.contains("192.168.4.25"));
        assert!(r.contains("192.168.4.0/24"));
    }

    #[test]
    fn redact_strips_jwt() {
        let line = "token=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ0ZXN0IiwiaWF0IjoxNjE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c finished";
        let r = redact_log_line(line);
        assert!(!r.contains("eyJzdWIiOiJ0ZXN0"));
        assert!(r.contains("<jwt>"));
    }

    #[test]
    fn redact_preserves_session_id_and_account_hash() {
        let line = "session_id=sess_abc123 account_hash=deadbeef0123 status=ok";
        let r = redact_log_line(line);
        assert!(r.contains("session_id=sess_abc123"));
        assert!(r.contains("deadbeef0123"));
    }

    #[test]
    fn redact_strips_device_code() {
        let line = "polling device_code=USER_ABC42 attempt=3";
        let r = redact_log_line(line);
        assert!(!r.contains("USER_ABC42"));
        assert!(r.contains("code=<redacted>"));
    }

    #[test]
    fn redact_masks_users_home_path() {
        let line = "log_dir=/Users/alice/Library/Logs/Bluey/daemon-log.2026-05-22.log";
        let r = redact_log_line(line);
        assert!(!r.contains("/Users/alice/"));
        assert!(r.contains("/Users/<redacted>/"));
        assert!(r.contains("/Library/Logs/Bluey/"));
    }

    #[test]
    fn redact_masks_linux_home_path() {
        let line = "config=/home/bob/.config/bluey/account.json";
        let r = redact_log_line(line);
        assert!(!r.contains("/home/bob/"));
        assert!(r.contains("/home/<redacted>/"));
    }

    #[test]
    fn redact_users_home_only_swaps_username() {
        let line = "stat /Users/uno/Library/Logs ok";
        let r = redact_log_line(line);
        assert!(!r.contains("/Users/uno/"));
        assert!(r.contains("/Users/<redacted>/Library"));
    }

    #[test]
    fn current_yyyymmdd_is_eight_digits() {
        let s = current_yyyymmdd();
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn default_output_path_includes_suffix() {
        let p = default_output_path(true);
        let name = p.file_name().unwrap().to_string_lossy();
        assert!(name.contains("redacted"));
        assert!(name.ends_with(".zip"));

        let p2 = default_output_path(false);
        let name2 = p2.file_name().unwrap().to_string_lossy();
        assert!(name2.contains("raw"));
    }
}
