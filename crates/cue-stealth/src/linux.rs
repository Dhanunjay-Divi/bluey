//! Linux process name masquerading via prctl(PR_SET_NAME).
//!
//! ## Limitations
//! - PR_SET_NAME is limited to 16 bytes (including null terminator), so only
//!   the first 15 characters of the name are used.
//! - This affects what shows in `top`, `htop`, and `/proc/self/comm`.
//! - The full command line in `/proc/self/cmdline` is NOT changed by this
//!   (would require overwriting argv[0] similar to macOS approach).

use crate::StealthError;

/// Set the process name via prctl. Truncates to 15 chars (kernel limit).
pub(crate) fn set_process_name(name: &str) -> Result<(), StealthError> {
    use std::ffi::CString;

    // Truncate to 15 bytes (PR_SET_NAME limit is 16 including null)
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
        // 20 chars - should truncate to 15 without error
        let result = set_process_name("Activity Monitor X");
        assert!(result.is_ok());
    }
}
