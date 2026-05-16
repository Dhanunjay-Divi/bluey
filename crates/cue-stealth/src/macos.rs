//! macOS process name masquerading via argv[0] overwrite + anti-debug helpers.
//!
//! ## Anti-debug
//! - `ptrace(PT_DENY_ATTACH)` prevents debuggers from attaching after startup.
//! - `is_debugger_attached()` checks the kernel `P_TRACED` flag via sysctl.
//!
//! **NOTE:** These are best-effort deterrents. A determined attacker with SIP
//! disabled or a kernel extension can bypass them. They raise the bar against
//! casual `lldb -p` inspection.
//!
//! ## Limitations (masquerading)
//! - Activity Monitor reads the Mach-O binary name at launch; overwriting argv[0]
//!   only affects `ps` output reliably.
//! - The CFBundleName env var trick works for the menu bar title but not for
//!   the dock or Activity Monitor binary name column.

use crate::StealthError;

extern "C" {
    fn _NSGetArgv() -> *mut *mut *mut libc::c_char;
    fn _NSGetArgc() -> *mut libc::c_int;
}

/// Overwrite argv[0] in-place with the given name.
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

        let orig_len = libc::strlen(arg0);
        if orig_len == 0 {
            return Ok(());
        }

        let name_bytes = name.as_bytes();
        let copy_len = name_bytes.len().min(orig_len);
        std::ptr::copy_nonoverlapping(name_bytes.as_ptr(), arg0 as *mut u8, copy_len);

        if copy_len < orig_len {
            std::ptr::write_bytes(arg0.add(copy_len) as *mut u8, 0, orig_len - copy_len);
        }
    }
    Ok(())
}

// ─── Anti-debug ──────────────────────────────────────────────────────────────

const PT_DENY_ATTACH: libc::c_int = 31;

/// Deny future debugger attachment via `ptrace(PT_DENY_ATTACH)`.
///
/// Returns `Ok(())` on success or if the call returns ENOTSUP (e.g. under
/// SIP-protected environments where the call is a no-op).
pub(crate) fn install_anti_debug() -> Result<(), StealthError> {
    let ret = unsafe { libc::ptrace(PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0) };
    if ret == -1 {
        let err = std::io::Error::last_os_error();
        // ENOTSUP is acceptable — means the OS doesn't support it in this context
        if err.raw_os_error() == Some(libc::ENOTSUP) {
            tracing::debug!("ptrace(PT_DENY_ATTACH) returned ENOTSUP — ignored");
            return Ok(());
        }
        return Err(StealthError::Ffi(format!("ptrace(PT_DENY_ATTACH): {err}")));
    }
    tracing::info!("anti-debug: PT_DENY_ATTACH installed");
    Ok(())
}

/// Check if a debugger is currently attached by inspecting the kernel proc flags.
///
/// Uses `sysctl([CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()])` and checks
/// the `p_flag` field for `P_TRACED` (0x800).
pub(crate) fn is_debugger_attached() -> bool {
    const P_TRACED: u32 = 0x00000800;
    // kinfo_proc is ~648 bytes on arm64 macOS; p_flag is at a known offset.
    // We allocate a buffer and read the flag at the correct offset.
    // On macOS arm64/x86_64, the p_flag field is at offset 32 within kp_proc,
    // and kp_proc starts at offset 0 of kinfo_proc.
    // Safer approach: use the full struct size and read p_flag at byte offset 32.
    const KINFO_PROC_SIZE: usize = 648;
    // p_flag offset: kp_proc.p_flag is at byte 32 in struct extern_proc
    const P_FLAG_OFFSET: usize = 32;

    let pid = unsafe { libc::getpid() };
    let mut mib: [libc::c_int; 4] = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];

    let mut buf = [0u8; KINFO_PROC_SIZE];
    let mut size: libc::size_t = KINFO_PROC_SIZE;

    let ret = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            4,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };

    if ret != 0 || size < P_FLAG_OFFSET + 4 {
        return false;
    }

    // Read p_flag as a little-endian i32 at the offset
    let flag_bytes: [u8; 4] = buf[P_FLAG_OFFSET..P_FLAG_OFFSET + 4]
        .try_into()
        .unwrap_or([0; 4]);
    let p_flag = u32::from_ne_bytes(flag_bytes);

    (p_flag & P_TRACED) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_process_name_does_not_crash() {
        let result = set_process_name("Test");
        assert!(result.is_ok());
    }

    #[test]
    fn set_process_name_long_name_truncates() {
        let long_name = "A".repeat(1024);
        let result = set_process_name(&long_name);
        assert!(result.is_ok());
    }

    #[test]
    fn anti_debug_install_succeeds() {
        // In test env (no debugger), should succeed or return ENOTSUP — both Ok.
        let result = install_anti_debug();
        assert!(result.is_ok());
    }

    #[test]
    fn is_debugger_attached_false_in_tests() {
        // cargo test does not run under a debugger
        assert!(!is_debugger_attached());
    }
}
