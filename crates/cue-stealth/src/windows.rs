//! Windows process masquerading via AppUserModelID + anti-debug helpers.
//!
//! ## Anti-debug
//! - `IsDebuggerPresent()` for local debugger detection.
//! - `CheckRemoteDebuggerPresent()` for parent-process debuggers.
//! - `install_anti_debug()` spawns a watchdog thread that polls every 5s and
//!   logs warnings if a debugger attaches mid-session. No process termination
//!   in v0.1 — too aggressive.
//!
//! **NOTE:** Best-effort. Determined attackers can patch IsDebuggerPresent or
//! clear the PEB BeingDebugged flag. This raises the bar against casual inspection.

use crate::StealthError;

static AUMID_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Set the App User Model ID for the current process.
pub(crate) fn set_app_user_model_id(aumid: &str) -> Result<(), StealthError> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    // The AppUserModelID is process-wide and the Shell setter is not safe to race.
    let _guard = AUMID_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let wide = HSTRING::from(aumid);
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(&wide).map_err(|e| {
            StealthError::Ffi(format!("SetCurrentProcessExplicitAppUserModelID: {e}"))
        })?;
    }
    Ok(())
}

// ─── Anti-debug ──────────────────────────────────────────────────────────────

/// Check if a debugger is attached (local or remote).
pub(crate) fn is_debugger_attached() -> bool {
    use windows::Win32::System::Diagnostics::Debug::{
        CheckRemoteDebuggerPresent, IsDebuggerPresent,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    // Local debugger check
    if unsafe { IsDebuggerPresent() }.as_bool() {
        return true;
    }

    // Remote debugger check
    let mut remote_present = windows::Win32::Foundation::BOOL(0);
    let ok = unsafe { CheckRemoteDebuggerPresent(GetCurrentProcess(), &mut remote_present) };
    if ok.is_ok() && remote_present.as_bool() {
        return true;
    }

    false
}

/// Spawn a watchdog thread that polls for debugger attachment every 5 seconds.
/// Logs a warning if detected — does NOT terminate (too aggressive for v0.1).
pub(crate) fn install_anti_debug() -> Result<(), StealthError> {
    std::thread::Builder::new()
        .name("anti-debug-watchdog".into())
        .spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if is_debugger_attached() {
                tracing::warn!("anti-debug: debugger detected mid-session!");
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
    fn set_aumid_valid() {
        let result = set_app_user_model_id("com.bluey.cue.terminal");
        assert!(result.is_ok());
    }

    #[test]
    fn concurrent_aumid_updates_are_serialized() {
        let workers = (0..8)
            .map(|index| {
                std::thread::spawn(move || {
                    let aumid = format!("com.bluey.cue.test{index}");
                    for _ in 0..8 {
                        set_app_user_model_id(&aumid).expect("AppUserModelID update failed");
                    }
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            worker.join().expect("AppUserModelID worker panicked");
        }
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
