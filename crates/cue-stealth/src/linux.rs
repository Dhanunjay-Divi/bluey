//! Linux process name masquerading via prctl(PR_SET_NAME) + anti-debug helpers.
//!
//! ## Anti-debug
//! - Reads `/proc/self/status` for `TracerPid` field. Non-zero = debugger.
//! - `install_anti_debug()` spawns a polling thread (5s interval) that logs
//!   warnings on debugger detection.
//!
//! **NOTE:** Best-effort. A root attacker can hide tracing or use ptrace
//! directly. This deters casual `gdb -p` attachment.

use crate::StealthError;

/// Set the process name via prctl. Truncates to 15 chars (kernel limit).
pub(crate) fn set_process_name(name: &str) -> Result<(), StealthError> {
    use std::ffi::CString;

    let truncated: String = name.chars().take(15).collect();
    let c_name =
        CString::new(truncated).map_err(|e| StealthError::Ffi(format!("invalid name: {e}")))?;

    let ret = unsafe { libc::prctl(libc::PR_SET_NAME, c_name.as_ptr(), 0, 0, 0) };
    if ret != 0 {
        return Err(StealthError::Ffi(format!(
            "prctl(PR_SET_NAME) returned {ret}"
        )));
    }
    Ok(())
}

// ─── Anti-debug ──────────────────────────────────────────────────────────────

/// Check if a debugger is attached by reading `/proc/self/status` TracerPid.
pub(crate) fn is_debugger_attached() -> bool {
    let status = match std::fs::read_to_string("/proc/self/status") {
        Ok(s) => s,
        Err(_) => return false,
    };

    for line in status.lines() {
        if let Some(val) = line.strip_prefix("TracerPid:") {
            let pid: i32 = val.trim().parse().unwrap_or(0);
            return pid != 0;
        }
    }
    false
}

/// Spawn a polling thread that checks for debugger attachment every 5 seconds.
pub(crate) fn install_anti_debug() -> Result<(), StealthError> {
    std::thread::Builder::new()
        .name("anti-debug-watchdog".into())
        .spawn(|| {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if is_debugger_attached() {
                    tracing::warn!("anti-debug: TracerPid non-zero — debugger detected!");
                }
            }
        })
        .map_err(|e| StealthError::Ffi(format!("failed to spawn watchdog: {e}")))?;

    tracing::info!("anti-debug: watchdog thread started (5s poll)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_process_name_short() {
        let result = set_process_name("Terminal");
        assert!(result.is_ok());
    }

    #[test]
    fn set_process_name_truncates_long() {
        let result = set_process_name("Activity Monitor X");
        assert!(result.is_ok());
    }

    #[test]
    fn is_debugger_attached_false_in_tests() {
        assert!(!is_debugger_attached());
    }

    #[test]
    fn install_anti_debug_succeeds() {
        let result = install_anti_debug();
        assert!(result.is_ok());
    }
}
