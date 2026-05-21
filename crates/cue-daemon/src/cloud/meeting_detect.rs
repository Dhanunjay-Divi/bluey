//! Codex Stage 18 commit 8: meeting-app heuristic for auto-disguise.
//!
//! MVP: poll NSWorkspace.frontmostApplication every 2s; if bundleID
//! matches a known meeting app, emit a "meeting_app_detected" event.
//! Production replaces with ScreenCaptureKit `SCStream` detection
//! once macOS minimum target is bumped.
//!
//! Cross-platform note: this MVP only fires on macOS. Windows + Linux
//! get a no-op until we wire DBus/UWP equivalents.

use std::sync::Arc;
use tokio::sync::watch;

const KNOWN_MEETING_BUNDLES: &[&str] = &[
    "us.zoom.xos",
    "com.microsoft.teams2",
    "com.microsoft.teams",
    "com.tinyspeck.slackmacgap",
    "com.cisco.webex.meetings",
    "com.google.Chrome.helper",   // best-effort for Meet PWA
    "company.thebrowser.Browser", // Arc browser
];

#[derive(Debug, Clone, serde::Serialize)]
pub struct MeetingAppEvent {
    pub bundle_id: String,
    pub detected_at_unix_ms: i64,
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
        let mut last_emitted: Option<String> = None;

        loop {
            interval.tick().await;
            let bundle = frontmost_bundle_id();
            if let Some(b) = bundle {
                let is_meeting = KNOWN_MEETING_BUNDLES
                    .iter()
                    .any(|known| b.starts_with(known));
                if is_meeting && last_emitted.as_deref() != Some(b.as_str()) {
                    let evt = MeetingAppEvent {
                        bundle_id: b.clone(),
                        detected_at_unix_ms: chrono::Utc::now().timestamp_millis(),
                    };
                    let _ = watcher.inner.send(Some(evt));
                    last_emitted = Some(b);
                }
            }
        }
    })
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_loop(_watcher: MeetingWatch) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {})
}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    // Use the `objc2` runtime that the rest of the daemon already uses
    // for NSWorkspace queries. For MVP we shell out to `lsappinfo`
    // because it works without adding a Cocoa dep here. Production swaps
    // to direct NSWorkspace.shared().frontmostApplication.
    let out = std::process::Command::new("lsappinfo")
        .arg("front")
        .output()
        .ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    // Output: ASN:0x0-0x12345678:"AppName"
    // Better: lsappinfo info -only bundleid <psn>
    // We re-run with -only bundleid:
    let psn = txt.lines().next()?.trim().to_string();
    if psn.is_empty() {
        return None;
    }
    let out2 = std::process::Command::new("lsappinfo")
        .args(["info", "-only", "bundleid", "front"])
        .output()
        .ok()?;
    let txt2 = String::from_utf8_lossy(&out2.stdout);
    // Output: "kCFBundleIdentifierKey"="us.zoom.xos"
    let idx = txt2.find(r#""kCFBundleIdentifierKey"=""#)?;
    let rest = &txt2[idx + r#""kCFBundleIdentifierKey"=""#.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
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
}
