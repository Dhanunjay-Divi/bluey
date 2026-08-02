//! Codex Stage 18 commit 8: meeting-app heuristic for auto-disguise.
//!
//! MVP: poll NSWorkspace.frontmostApplication every 2s; if bundleID
//! matches a known meeting app, emit a "meeting_app_detected" event.
//! Production replaces with ScreenCaptureKit `SCStream` detection
//! once macOS minimum target is bumped.
//!
//! On Windows, Bluey polls active Core Audio capture sessions and reduces
//! process image names to a small, content-free meeting-app allowlist. The
//! Windows path never reads captured audio and never emits paths or PIDs.

#[cfg(any(target_os = "windows", test))]
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use tokio::sync::watch;

#[cfg(any(target_os = "macos", test))]
const KNOWN_MEETING_BUNDLES: &[&str] = &[
    "us.zoom.xos",
    "com.microsoft.teams2",
    "com.microsoft.teams",
    "com.tinyspeck.slackmacgap",
    "com.cisco.webex.meetings",
    "com.google.Chrome.helper",   // best-effort for Meet PWA
    "company.thebrowser.Browser", // Arc browser
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MeetingAppEvent {
    pub bundle_id: String,
    pub detected_at_unix_ms: i64,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum MeetingDetectionUpdate {
    Detected(MeetingAppEvent),
    Cleared,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WindowsMeetingHint {
    Teams,
    Zoom,
    Webex,
    Slack,
    Discord,
    MeetBrowser,
}

#[cfg(any(target_os = "windows", test))]
impl WindowsMeetingHint {
    fn event_id(self) -> &'static str {
        match self {
            Self::Teams => "windows.audio-capture.teams",
            Self::Zoom => "windows.audio-capture.zoom",
            Self::Webex => "windows.audio-capture.webex",
            Self::Slack => "windows.audio-capture.slack",
            Self::Discord => "windows.audio-capture.discord",
            // A browser capture session is only a Meet-compatible browser hint;
            // process identity alone cannot prove which site owns the microphone.
            Self::MeetBrowser => "windows.audio-capture.meet-browser-hint",
        }
    }

    fn is_browser(self) -> bool {
        self == Self::MeetBrowser
    }
}

#[cfg(any(target_os = "windows", test))]
fn classify_windows_process_image(image_name: &str) -> Option<WindowsMeetingHint> {
    let file_name = image_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(image_name)
        .to_ascii_lowercase();

    match file_name.as_str() {
        "ms-teams.exe" | "teams.exe" => Some(WindowsMeetingHint::Teams),
        "zoom.exe" => Some(WindowsMeetingHint::Zoom),
        "webex.exe" | "ciscocollabhost.exe" | "webexhost.exe" => Some(WindowsMeetingHint::Webex),
        "slack.exe" => Some(WindowsMeetingHint::Slack),
        "discord.exe" | "discordcanary.exe" | "discordptb.exe" => Some(WindowsMeetingHint::Discord),
        // These are deliberately hints, not assertions that Google Meet is open.
        "chrome.exe" | "msedge.exe" | "firefox.exe" | "brave.exe" => {
            Some(WindowsMeetingHint::MeetBrowser)
        }
        _ => None,
    }
}

#[cfg(test)]
fn classify_windows_process_samples(
    samples: impl IntoIterator<Item = (u32, String)>,
) -> BTreeSet<WindowsMeetingHint> {
    let mut seen_pids = HashSet::new();
    samples
        .into_iter()
        .filter(|(pid, _)| *pid != 0 && seen_pids.insert(*pid))
        .filter_map(|(_, image_name)| classify_windows_process_image(&image_name))
        .collect()
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Default)]
struct WindowsMeetingDetectionState {
    active: Option<WindowsMeetingHint>,
    pending: Option<WindowsMeetingHint>,
    pending_samples: u8,
    missing_samples: u8,
}

#[cfg(any(target_os = "windows", test))]
impl WindowsMeetingDetectionState {
    const NATIVE_START_SAMPLES: u8 = 2;
    const BROWSER_START_SAMPLES: u8 = 3;
    const NATIVE_STOP_SAMPLES: u8 = 3;
    const BROWSER_STOP_SAMPLES: u8 = 5;

    fn observe_success(
        &mut self,
        hints: &BTreeSet<WindowsMeetingHint>,
        detected_at_unix_ms: i64,
    ) -> Option<MeetingDetectionUpdate> {
        // The enum order puts exact native-app identities ahead of the weaker
        // browser hint. Multiple sessions for one app collapse to one hint.
        let candidate = hints.iter().next().copied();

        if candidate == self.active {
            self.pending = None;
            self.pending_samples = 0;
            self.missing_samples = 0;
            return None;
        }

        if let Some(candidate) = candidate {
            self.missing_samples = 0;
            if self.pending == Some(candidate) {
                self.pending_samples = self.pending_samples.saturating_add(1);
            } else {
                self.pending = Some(candidate);
                self.pending_samples = 1;
            }

            let required = if candidate.is_browser() {
                Self::BROWSER_START_SAMPLES
            } else {
                Self::NATIVE_START_SAMPLES
            };
            if self.pending_samples < required {
                return None;
            }

            self.active = Some(candidate);
            self.pending = None;
            self.pending_samples = 0;
            return Some(MeetingDetectionUpdate::Detected(MeetingAppEvent {
                bundle_id: candidate.event_id().to_string(),
                detected_at_unix_ms,
            }));
        }

        self.pending = None;
        self.pending_samples = 0;
        let active = self.active?;
        self.missing_samples = self.missing_samples.saturating_add(1);
        let required = if active.is_browser() {
            Self::BROWSER_STOP_SAMPLES
        } else {
            Self::NATIVE_STOP_SAMPLES
        };
        if self.missing_samples < required {
            return None;
        }

        self.active = None;
        self.missing_samples = 0;
        Some(MeetingDetectionUpdate::Cleared)
    }

    /// An incomplete scan is not evidence for either starting or stopping.
    fn observe_ambiguous(&mut self) {
        self.pending = None;
        self.pending_samples = 0;
        self.missing_samples = 0;
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Default)]
struct MeetingDetectionState {
    active_bundle_id: Option<String>,
}

#[cfg(any(target_os = "macos", test))]
impl MeetingDetectionState {
    fn observe(
        &mut self,
        frontmost_bundle_id: Option<String>,
        detected_at_unix_ms: i64,
    ) -> Option<MeetingDetectionUpdate> {
        let detected_bundle = frontmost_bundle_id.filter(|bundle_id| {
            KNOWN_MEETING_BUNDLES
                .iter()
                .any(|known| bundle_id.starts_with(known))
        });

        if self.active_bundle_id == detected_bundle {
            return None;
        }

        self.active_bundle_id = detected_bundle.clone();
        match detected_bundle {
            Some(bundle_id) => Some(MeetingDetectionUpdate::Detected(MeetingAppEvent {
                bundle_id,
                detected_at_unix_ms,
            })),
            None => Some(MeetingDetectionUpdate::Cleared),
        }
    }
}

#[derive(Clone)]
pub struct MeetingWatch {
    inner: Arc<watch::Sender<Option<MeetingAppEvent>>>,
}

impl Default for MeetingWatch {
    fn default() -> Self {
        Self {
            inner: Arc::new(watch::channel(None).0),
        }
    }
}

impl MeetingWatch {
    pub fn current(&self) -> Option<MeetingAppEvent> {
        self.inner.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Option<MeetingAppEvent>> {
        self.inner.subscribe()
    }
}

#[cfg(target_os = "macos")]
pub fn spawn_loop(watcher: MeetingWatch) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut detection = MeetingDetectionState::default();

        loop {
            interval.tick().await;
            match detection.observe(frontmost_bundle_id(), chrono::Utc::now().timestamp_millis()) {
                Some(MeetingDetectionUpdate::Detected(event)) => {
                    watcher.inner.send_replace(Some(event));
                }
                Some(MeetingDetectionUpdate::Cleared) => {
                    watcher.inner.send_replace(None);
                }
                None => {}
            }
        }
    })
}

#[cfg(target_os = "windows")]
pub fn spawn_loop(watcher: MeetingWatch) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut detection = WindowsMeetingDetectionState::default();

        loop {
            interval.tick().await;
            let scan = tokio::task::spawn_blocking(windows_capture_hints).await;
            let update = match scan {
                Ok(Ok(hints)) => {
                    detection.observe_success(&hints, chrono::Utc::now().timestamp_millis())
                }
                Ok(Err(())) | Err(_) => {
                    // Do not include HRESULTs, process paths, or PIDs in logs.
                    tracing::debug!("Windows microphone-session hint scan was inconclusive");
                    detection.observe_ambiguous();
                    None
                }
            };

            match update {
                Some(MeetingDetectionUpdate::Detected(event)) => {
                    watcher.inner.send_replace(Some(event));
                }
                Some(MeetingDetectionUpdate::Cleared) => {
                    watcher.inner.send_replace(None);
                }
                None => {}
            }
        }
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn spawn_loop(_watcher: MeetingWatch) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {})
}

#[cfg(target_os = "windows")]
fn windows_capture_hints() -> Result<BTreeSet<WindowsMeetingHint>, ()> {
    use windows::core::Interface;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Media::Audio::{
        eCapture, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
        IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            // SAFETY: the guard exists only after this thread successfully
            // called CoInitializeEx, and it is dropped on that same thread.
            unsafe { CoUninitialize() };
        }
    }

    struct ProcessHandle(HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            // SAFETY: OpenProcess returned this owned, non-inheritable handle.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    fn process_image_name(pid: u32) -> Result<String, ()> {
        // SAFETY: the access mask is query-only and handle inheritance is disabled.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map(ProcessHandle)
            .map_err(|_| ())?;
        let mut image = vec![0_u16; 32_768];
        let mut length = image.len() as u32;
        // SAFETY: `image` is writable for `length` UTF-16 units; the handle
        // remains live for the call and the API updates length on success.
        unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(image.as_mut_ptr()),
                &mut length,
            )
        }
        .map_err(|_| ())?;
        image.truncate(length as usize);
        let path = String::from_utf16(&image).map_err(|_| ())?;
        Ok(path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(path.as_str())
            .to_string())
    }

    // SAFETY: initialization and teardown are paired by ComGuard on this
    // spawn_blocking thread. A changed apartment model is treated as ambiguous.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|_| ())?;
    let _com = ComGuard;

    // SAFETY: all returned COM interfaces remain within this initialized thread.
    let device_enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|_| ())?;
    let devices = unsafe { device_enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }
        .map_err(|_| ())?;

    let device_count = unsafe { devices.GetCount() }.map_err(|_| ())?;
    let mut active_pids = HashSet::new();
    let mut unresolved_active_session = false;

    for device_index in 0..device_count {
        let device = unsafe { devices.Item(device_index) }.map_err(|_| ())?;
        let manager: IAudioSessionManager2 =
            unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|_| ())?;
        let sessions = unsafe { manager.GetSessionEnumerator() }.map_err(|_| ())?;
        let session_count = unsafe { sessions.GetCount() }.map_err(|_| ())?;

        for session_index in 0..session_count {
            let control = match unsafe { sessions.GetSession(session_index) } {
                Ok(control) => control,
                Err(_) => {
                    unresolved_active_session = true;
                    continue;
                }
            };
            match unsafe { control.GetState() } {
                Ok(state) if state == AudioSessionStateActive => {}
                Ok(_) => continue,
                Err(_) => {
                    unresolved_active_session = true;
                    continue;
                }
            }
            let control2: IAudioSessionControl2 = match control.cast() {
                Ok(control) => control,
                Err(_) => {
                    unresolved_active_session = true;
                    continue;
                }
            };
            match unsafe { control2.GetProcessId() } {
                Ok(pid) if pid != 0 => {
                    active_pids.insert(pid);
                }
                Ok(_) => {}
                Err(_) => unresolved_active_session = true,
            }
        }
    }

    let mut hints = BTreeSet::new();
    for pid in active_pids {
        match process_image_name(pid) {
            Ok(image_name) => {
                if let Some(hint) = classify_windows_process_image(&image_name) {
                    hints.insert(hint);
                }
            }
            Err(()) => unresolved_active_session = true,
        }
    }

    // A recognized allowlisted process is positive evidence even if an unrelated
    // session raced with enumeration. With no recognized process, any unresolved
    // session makes the negative result ambiguous and therefore non-actionable.
    if hints.is_empty() && unresolved_active_session {
        Err(())
    } else {
        Ok(hints)
    }
}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    // Codex review remaining nit closure: direct NSWorkspace query
    // instead of shelling out to lsappinfo. No fork+exec, no fragile
    // string parsing.
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;

    autoreleasepool(|_| unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        app.bundleIdentifier().map(|s| s.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn watch_default_is_empty() {
        let w = MeetingWatch::default();
        assert!(w.current().is_none());
    }

    #[tokio::test]
    async fn watch_publishes_meeting_event() {
        let w = MeetingWatch::default();
        let mut rx = w.subscribe();
        let evt = MeetingAppEvent {
            bundle_id: "us.zoom.xos".to_string(),
            detected_at_unix_ms: 1700000000000,
        };
        w.inner.send(Some(evt.clone())).unwrap();
        rx.changed().await.unwrap();
        assert_eq!(rx.borrow().as_ref().unwrap().bundle_id, "us.zoom.xos");
    }

    #[test]
    fn known_bundles_includes_zoom_teams_slack() {
        assert!(KNOWN_MEETING_BUNDLES.contains(&"us.zoom.xos"));
        assert!(KNOWN_MEETING_BUNDLES.iter().any(|b| b.contains("teams")));
        assert!(KNOWN_MEETING_BUNDLES.iter().any(|b| b.contains("slack")));
    }

    #[test]
    fn meeting_detection_clears_and_reemits_same_app_after_inactive_gap() {
        let mut state = MeetingDetectionState::default();

        assert_eq!(
            state.observe(Some("us.zoom.xos".to_string()), 100),
            Some(MeetingDetectionUpdate::Detected(MeetingAppEvent {
                bundle_id: "us.zoom.xos".to_string(),
                detected_at_unix_ms: 100,
            }))
        );
        assert_eq!(state.observe(Some("us.zoom.xos".to_string()), 101), None);
        assert_eq!(
            state.observe(Some("com.apple.TextEdit".to_string()), 102),
            Some(MeetingDetectionUpdate::Cleared)
        );
        assert_eq!(
            state.observe(Some("us.zoom.xos".to_string()), 103),
            Some(MeetingDetectionUpdate::Detected(MeetingAppEvent {
                bundle_id: "us.zoom.xos".to_string(),
                detected_at_unix_ms: 103,
            }))
        );
    }

    #[test]
    fn meeting_detection_switches_directly_between_meeting_apps() {
        let mut state = MeetingDetectionState::default();
        state.observe(Some("us.zoom.xos".to_string()), 100);

        assert_eq!(
            state.observe(Some("com.microsoft.teams2".to_string()), 101),
            Some(MeetingDetectionUpdate::Detected(MeetingAppEvent {
                bundle_id: "com.microsoft.teams2".to_string(),
                detected_at_unix_ms: 101,
            }))
        );
    }

    #[test]
    fn meeting_detection_does_not_emit_repeated_inactive_updates() {
        let mut state = MeetingDetectionState::default();
        assert_eq!(state.observe(None, 100), None);
        assert_eq!(
            state.observe(Some("com.apple.TextEdit".to_string()), 101),
            None
        );
    }

    #[test]
    fn windows_process_allowlist_is_exact_and_content_free() {
        assert_eq!(
            classify_windows_process_image(r"C:\\Program Files\\Zoom\\Zoom.exe"),
            Some(WindowsMeetingHint::Zoom)
        );
        assert_eq!(
            classify_windows_process_image("ms-teams.exe"),
            Some(WindowsMeetingHint::Teams)
        );
        assert_eq!(
            classify_windows_process_image("ciscocollabhost.exe"),
            Some(WindowsMeetingHint::Webex)
        );
        assert_eq!(
            classify_windows_process_image("chrome.exe"),
            Some(WindowsMeetingHint::MeetBrowser)
        );
        assert_eq!(classify_windows_process_image("zoom-helper.exe"), None);
        assert_eq!(classify_windows_process_image("notepad.exe"), None);
        assert!(!WindowsMeetingHint::Zoom.event_id().contains(".exe"));
    }

    #[test]
    fn windows_samples_deduplicate_pids_and_keep_a_browser_hint_during_process_churn() {
        let first = classify_windows_process_samples([
            (10, "chrome.exe".to_string()),
            (10, "chrome.exe".to_string()),
            (20, "msedge.exe".to_string()),
        ]);
        let after_one_browser_session_disappears =
            classify_windows_process_samples([(20, "msedge.exe".to_string())]);

        assert_eq!(first, BTreeSet::from([WindowsMeetingHint::MeetBrowser]));
        assert_eq!(after_one_browser_session_disappears, first);
    }

    #[test]
    fn windows_native_hint_requires_start_and_stop_debounce() {
        let zoom = BTreeSet::from([WindowsMeetingHint::Zoom]);
        let empty = BTreeSet::new();
        let mut state = WindowsMeetingDetectionState::default();

        assert_eq!(state.observe_success(&zoom, 100), None);
        assert!(matches!(
            state.observe_success(&zoom, 101),
            Some(MeetingDetectionUpdate::Detected(_))
        ));
        assert_eq!(state.observe_success(&empty, 102), None);
        assert_eq!(state.observe_success(&empty, 103), None);
        assert_eq!(
            state.observe_success(&empty, 104),
            Some(MeetingDetectionUpdate::Cleared)
        );
    }

    #[test]
    fn windows_browser_hint_uses_stricter_start_and_stop_debounce() {
        let browser = BTreeSet::from([WindowsMeetingHint::MeetBrowser]);
        let empty = BTreeSet::new();
        let mut state = WindowsMeetingDetectionState::default();

        assert_eq!(state.observe_success(&browser, 100), None);
        assert_eq!(state.observe_success(&browser, 101), None);
        assert!(matches!(
            state.observe_success(&browser, 102),
            Some(MeetingDetectionUpdate::Detected(_))
        ));
        for timestamp in 103..107 {
            assert_eq!(state.observe_success(&empty, timestamp), None);
        }
        assert_eq!(
            state.observe_success(&empty, 107),
            Some(MeetingDetectionUpdate::Cleared)
        );
    }

    #[test]
    fn windows_ambiguous_scan_cannot_start_or_stop_detection() {
        let zoom = BTreeSet::from([WindowsMeetingHint::Zoom]);
        let empty = BTreeSet::new();
        let mut state = WindowsMeetingDetectionState::default();

        assert_eq!(state.observe_success(&zoom, 100), None);
        state.observe_ambiguous();
        assert_eq!(state.observe_success(&zoom, 101), None);
        assert!(matches!(
            state.observe_success(&zoom, 102),
            Some(MeetingDetectionUpdate::Detected(_))
        ));

        assert_eq!(state.observe_success(&empty, 103), None);
        state.observe_ambiguous();
        assert_eq!(state.observe_success(&empty, 104), None);
        assert_eq!(state.observe_success(&empty, 105), None);
        assert_eq!(
            state.observe_success(&empty, 106),
            Some(MeetingDetectionUpdate::Cleared)
        );
    }
}
