//! macOS permission probes for `bluey doctor`.
//!
//! Three probes:
//! * Accessibility — `AXIsProcessTrusted()` from ApplicationServices.
//! * Microphone — `[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]`
//!   via objc2 msg_send.
//! * Screen Recording — `CGPreflightScreenCaptureAccess()` from CoreGraphics
//!   (macOS 11+).
//!
//! Each probe returns a `PermissionStatus` tristate. The probes call
//! into TCC indirectly: they do NOT prompt the user, just report the
//! current state.
//!
//! On non-macOS targets, all probes return `NotApplicable`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionStatus {
    /// Permission is granted and ready to use.
    Granted,
    /// Permission was explicitly denied by the user.
    Denied,
    /// Permission has not been requested yet (TCC has no record).
    NotDetermined,
    /// Permission is restricted (e.g., parental controls); not user-toggleable.
    Restricted,
    /// Probe failed at runtime (e.g., framework not available).
    Unknown,
    /// Not applicable (non-macOS platform).
    NotApplicable,
}

impl PermissionStatus {
    pub fn label(self) -> &'static str {
        match self {
            PermissionStatus::Granted => "granted",
            PermissionStatus::Denied => "denied",
            PermissionStatus::NotDetermined => "not requested yet",
            PermissionStatus::Restricted => "restricted (parental controls / MDM)",
            PermissionStatus::Unknown => "probe failed",
            PermissionStatus::NotApplicable => "n/a (non-macOS)",
        }
    }

    pub fn hint(self, what: &str) -> Option<String> {
        match self {
            PermissionStatus::Denied => Some(format!(
                "open System Settings → Privacy & Security → {what} to grant Bluey",
            )),
            PermissionStatus::NotDetermined => Some(format!(
                "{what} permission has not been prompted; Bluey will request it on first use",
            )),
            PermissionStatus::Restricted => Some(format!(
                "{what} access is restricted by your system administrator",
            )),
            _ => None,
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::PermissionStatus;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        // macOS 11.0+. Reports current screen-capture permission without
        // prompting. Returns true if granted, false if not.
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    // AVFoundation is needed so the AVCaptureDevice Objective-C class
    // is loaded into the runtime when microphone_status() does the
    // class lookup. Without this link the class lookup returns None
    // even on macOS systems that have AVFoundation installed.
    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {}

    pub fn accessibility_status() -> PermissionStatus {
        // SAFETY: AXIsProcessTrusted is a parameterless C function in
        // ApplicationServices; well-defined to call from any thread.
        let trusted = unsafe { AXIsProcessTrusted() };
        if trusted {
            PermissionStatus::Granted
        } else {
            // AX has no tristate; either the user has trusted us or not.
            // We can't distinguish "denied" from "not yet asked" here.
            PermissionStatus::NotDetermined
        }
    }

    pub fn screen_recording_status() -> PermissionStatus {
        // SAFETY: CGPreflightScreenCaptureAccess is a parameterless
        // C function. macOS 11+ only; on older systems the function
        // may be unresolved at runtime — link will fail. Bluey
        // requires macOS 12+ per the support matrix, so this is fine.
        let granted = unsafe { CGPreflightScreenCaptureAccess() };
        if granted {
            PermissionStatus::Granted
        } else {
            PermissionStatus::NotDetermined
        }
    }

    pub fn microphone_status() -> PermissionStatus {
        // [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]
        // returns AVAuthorizationStatus (NSInteger):
        //   0 = NotDetermined
        //   1 = Restricted
        //   2 = Denied
        //   3 = Authorized
        //
        // AVMediaTypeAudio is an NSString constant equal to @"soun"
        // (a 4-char-code FourCC). We construct that NSString from
        // the literal.
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::NSString;

        let Some(class) = objc2::runtime::AnyClass::get("AVCaptureDevice") else {
            return PermissionStatus::Unknown;
        };
        let media_type = NSString::from_str("soun");
        let status: i64 = unsafe {
            let media_ptr: *const NSString = &*media_type;
            msg_send![
                class,
                authorizationStatusForMediaType: media_ptr as *const AnyObject
            ]
        };
        match status {
            0 => PermissionStatus::NotDetermined,
            1 => PermissionStatus::Restricted,
            2 => PermissionStatus::Denied,
            3 => PermissionStatus::Granted,
            _ => PermissionStatus::Unknown,
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos_impl {
    use super::PermissionStatus;

    pub fn accessibility_status() -> PermissionStatus {
        PermissionStatus::NotApplicable
    }
    pub fn microphone_status() -> PermissionStatus {
        PermissionStatus::NotApplicable
    }
    pub fn screen_recording_status() -> PermissionStatus {
        PermissionStatus::NotApplicable
    }
}

pub use macos_impl::{accessibility_status, microphone_status, screen_recording_status};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_strings_are_distinct() {
        // Sanity: every variant produces a unique non-empty label.
        let labels = [
            PermissionStatus::Granted.label(),
            PermissionStatus::Denied.label(),
            PermissionStatus::NotDetermined.label(),
            PermissionStatus::Restricted.label(),
            PermissionStatus::Unknown.label(),
            PermissionStatus::NotApplicable.label(),
        ];
        for l in labels {
            assert!(!l.is_empty());
        }
        let mut sorted = labels.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "labels must be unique");
    }

    #[test]
    fn hint_returns_some_only_for_actionable_states() {
        assert!(PermissionStatus::Granted.hint("Microphone").is_none());
        assert!(PermissionStatus::Denied.hint("Microphone").is_some());
        assert!(PermissionStatus::NotDetermined.hint("Microphone").is_some());
        assert!(PermissionStatus::Restricted.hint("Microphone").is_some());
        assert!(PermissionStatus::Unknown.hint("Microphone").is_none());
        assert!(PermissionStatus::NotApplicable.hint("Microphone").is_none());
    }

    /// Each probe must return *some* status without panicking on the
    /// current platform. We can't assert specific values because they
    /// depend on the OS state of the test runner.
    #[test]
    fn probes_are_callable_without_panicking() {
        let _ = accessibility_status();
        let _ = microphone_status();
        let _ = screen_recording_status();
    }
}
