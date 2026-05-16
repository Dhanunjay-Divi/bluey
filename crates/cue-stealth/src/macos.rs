//! macOS process name masquerading via argv[0] overwrite.
//!
//! ## Limitations
//! - Activity Monitor reads the Mach-O binary name at launch; overwriting argv[0]
//!   only affects `ps` output reliably. Apple has progressively locked down
//!   process-title mutation since macOS 11.
//! - The CFBundleName env var trick works for the menu bar title but not for
//!   the dock or Activity Monitor binary name column.
//! - True dock-icon hiding requires NSApplication activation policy changes
//!   which are deferred to a follow-up (requires objc runtime calls).

use crate::StealthError;

extern "C" {
    fn _NSGetArgv() -> *mut *mut *mut libc::c_char;
    fn _NSGetArgc() -> *mut libc::c_int;
}

/// Overwrite argv[0] in-place with the given name.
///
/// This is best-effort: the new name is truncated to the original argv[0]
/// length and null-padded. Works for `ps aux` display on macOS.
pub(crate) fn set_process_name(name: &str) -> Result<(), StealthError> {
    unsafe {
        let argv_ptr = _NSGetArgv();
        if argv_ptr.is_null() {
            return Err(StealthError::Ffi("_NSGetArgv returned null".into()));
        }
        let argv = *argv_ptr;
        if argv.is_null() {
            return Err(StealthError::Ffi("argv is null".into()));
        }
        let arg0 = *argv;
        if arg0.is_null() {
            return Err(StealthError::Ffi("argv[0] is null".into()));
        }

        // Determine original length of argv[0]
        let orig_len = libc::strlen(arg0);
        if orig_len == 0 {
            return Ok(());
        }

        // Write new name, truncated to original length
        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(orig_len);
        std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), arg0 as *mut u8, copy_len);

        // Null-pad remainder
        if copy_len < orig_len {
            std::ptr::write_bytes(arg0.add(copy_len) as *mut u8, 0, orig_len - copy_len);
        }
    }
    Ok(())
}

// TODO(p3r8-followup): NSApplication.setActivationPolicy(.prohibited) to hide
// from dock entirely. Requires objc2 crate or raw objc_msgSend FFI. Deferred
// because it interacts with Tauri's own dock management.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_process_name_does_not_crash() {
        // Just verify it doesn't segfault with a short name
        let result = set_process_name("Test");
        assert!(result.is_ok());
    }

    #[test]
    fn set_process_name_long_name_truncates() {
        // A very long name should not crash (gets truncated)
        let long_name = "A".repeat(1024);
        let result = set_process_name(&long_name);
        assert!(result.is_ok());
    }
}
