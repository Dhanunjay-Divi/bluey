//! Windows process masquerading via AppUserModelID.
//!
//! ## Limitations
//! - SetCurrentProcessExplicitAppUserModelID affects taskbar grouping and
//!   jump lists but does NOT change the process name in Task Manager.
//! - Changing the Task Manager display name would require modifying the PE
//!   version info resource at build time, which is out of scope for runtime.
//! - The window title (set via Tauri) is what users see in the taskbar tooltip.

use crate::StealthError;

/// Set the App User Model ID for the current process.
/// This controls Windows taskbar grouping and notification identity.
pub(crate) fn set_app_user_model_id(aumid: &str) -> Result<(), StealthError> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let wide = HSTRING::from(aumid);
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(&wide).map_err(|e| {
            StealthError::Ffi(format!("SetCurrentProcessExplicitAppUserModelID: {e}"))
        })?;
    }
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
}
