//! Calendar trigger — fires the warm meeting-backend drive ahead of an
//! upcoming meeting (the pivot's ONLY trigger; there is no listen-start hook).
//!
//! Deterministic Rust owns the clock: a poll loop scans a rolling look-ahead
//! window and fires `WarmupStart` once per (event id, occurrence) at
//! T-minus [`WARM_LEAD_SECS`]. The scan source is pluggable:
//!
//! - **EventKit** (feature `calendar`, macOS): the real system calendar via
//!   `objc2-event-kit`. Requires the user's TCC consent
//!   (`NSCalendarsFullAccessUsageDescription` in the packaged app); denied or
//!   undecided access degrades to "no events" — never a crash, never a
//!   prompt loop.
//! - **Env fake** (`BLUEY_CALENDAR_FAKE_EVENTS`): `title@epoch_secs[;…]` —
//!   the same test-hook pattern as `BLUEY_AUDIO_WAV_FILE`, driving the FULL
//!   trigger path headless (poll → dedupe → warm fire) with zero OS deps.
//!
//! Idempotency: a fired set keyed by `(event_id, occurrence_start)` so
//! rescheduled meetings re-arm (new occurrence key) while poll ticks never
//! double-fire. Back-to-back meetings each fire their own warmup; the
//! warm-drive itself is meeting-scoped (create-iff-none).

use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// The shared calendar seam types now live in `cue-core` so the out-of-daemon
// cloud calendar crate (`cue-calendar-cloud`) can implement `CalendarSource`
// without a dependency cycle. Everything below (EventKit / env-fake / no-op
// sources, dedupe, and the warm-fire trigger core) stays in the daemon.
pub use cue_core::calendar::{CalendarSource, Participant, UpcomingEvent};

/// Fire the warm drive this many seconds before the event starts.
pub const WARM_LEAD_SECS: u64 = 180;
/// Rolling look-ahead window the poll scans.
pub const LOOKAHEAD_SECS: u64 = 600;
/// Poll cadence.
pub const POLL_SECS: u64 = 30;


/// A source that never yields events — the fallback when neither the env fake
/// nor a real calendar backend is available.
pub struct NoopSource;

impl CalendarSource for NoopSource {
    fn upcoming(&self, _now: u64) -> Vec<UpcomingEvent> {
        Vec::new()
    }
}

/// Pick the calendar source the daemon should poll, in priority order:
/// 1. the env fake when `BLUEY_CALENDAR_FAKE_EVENTS` is set (the deterministic
///    test hook wins first so a test never races a real calendar);
/// 2. a connected cloud OAuth calendar (feature `cloud-calendar`) — Google, then
///    Microsoft — when that provider has tokens stored in its keychain;
/// 3. the real EventKit source (feature `calendar`, macOS);
/// 4. a no-op (the trigger stays dormant rather than erroring).
///
/// The cloud branch needs the daemon's tokio [`Handle`] to spawn the source's
/// background refresh task. `default_source()` is called from within the daemon's
/// calendar-poll task (an async context — see `app.rs`), so
/// [`Handle::try_current`] resolves it without a signature change; if this were
/// ever called off the runtime, the cloud branch is skipped (falling through to
/// Noop) rather than panicking.
///
/// Sources, in priority order: the `BLUEY_CALENDAR_FAKE_EVENTS` test hook, then a
/// connected cloud provider (Google / Microsoft OAuth — feature `cloud-calendar`),
/// else a no-op. (Apple EventKit was removed: it only sees calendars the user
/// added to macOS Calendar, so it's blind for most users and macOS-only — the
/// cloud OAuth path reaches the real account and is cross-platform.)
pub fn default_source() -> Box<dyn CalendarSource> {
    if std::env::var("BLUEY_CALENDAR_FAKE_EVENTS").is_ok() {
        return Box::new(EnvFakeSource);
    }
    #[cfg(feature = "cloud-calendar")]
    {
        if let Some(source) = cloud_source() {
            return source;
        }
    }
    Box::new(NoopSource)
}

/// Build a cloud calendar source when a provider is connected (tokens present in
/// its per-provider keychain), preferring Google, then Microsoft. Returns `None`
/// when neither is connected or when no tokio runtime handle is available (so the
/// caller falls through to EventKit / no-op). Keychain-read errors are treated as
/// "not connected" (fail-soft) — never a panic.
#[cfg(feature = "cloud-calendar")]
fn cloud_source() -> Option<Box<dyn CalendarSource>> {
    use cue_calendar_cloud::google::GoogleCalendarSource;
    use cue_calendar_cloud::microsoft::MicrosoftCalendarSource;
    use cue_calendar_cloud::{CalTokenStore, KeyringCalStore, Provider};
    use std::sync::Arc;

    // Spawning the source's background refresh task needs a runtime handle; if
    // we're somehow off the runtime, skip the cloud branch rather than panic.
    let handle = tokio::runtime::Handle::try_current().ok()?;

    // A provider is "connected" iff its keychain store holds tokens. A keyring
    // error reads as not-connected (fail-soft) instead of failing the pick.
    let is_connected = |provider: Provider| -> bool {
        matches!(
            KeyringCalStore::new(provider.keyring_service()).load(),
            Ok(Some(_))
        )
    };

    if is_connected(Provider::Google) {
        let store: Arc<dyn CalTokenStore> =
            Arc::new(KeyringCalStore::new(Provider::Google.keyring_service()));
        return Some(Box::new(GoogleCalendarSource::spawn(store, handle)));
    }
    if is_connected(Provider::Microsoft) {
        let store: Arc<dyn CalTokenStore> =
            Arc::new(KeyringCalStore::new(Provider::Microsoft.keyring_service()));
        return Some(Box::new(MicrosoftCalendarSource::spawn(store, handle)));
    }
    None
}

/// The env-driven fake (`BLUEY_CALENDAR_FAKE_EVENTS="Standup@1783560000;…"`).
pub struct EnvFakeSource;

impl CalendarSource for EnvFakeSource {
    fn upcoming(&self, now: u64) -> Vec<UpcomingEvent> {
        let Ok(spec) = std::env::var("BLUEY_CALENDAR_FAKE_EVENTS") else {
            return Vec::new();
        };
        spec.split(';')
            .filter_map(|entry| {
                let (title, start) = entry.trim().split_once('@')?;
                let start: u64 = start.trim().parse().ok()?;
                Some(UpcomingEvent {
                    id: format!("fake-{}", title.trim()),
                    title: title.trim().to_string(),
                    start_epoch_secs: start,
                    participants: Vec::new(), // the fake carries no roster
                    ..Default::default()
                })
            })
            .filter(|e| e.start_epoch_secs >= now && e.start_epoch_secs <= now + LOOKAHEAD_SECS)
            .collect()
    }
}

/// The once-per-occurrence dedupe key: `(event id, occurrence start)` — a
/// MOVED event gets a new key and re-arms.
pub fn fired_key(event: &UpcomingEvent) -> (String, u64) {
    (event.id.clone(), event.start_epoch_secs)
}

/// Pure trigger core: which events are due to warm NOW, given the fired-set.
/// READ-ONLY on the set — the caller inserts [`fired_key`] only when the warm
/// open SUCCEEDS, so a refused open (agent not attached yet, server down)
/// retries every tick until the meeting starts and leaves the window. Kept
/// free of IO/time so the once-per-occurrence contract is unit-testable.
pub fn due_for_warmup(
    events: &[UpcomingEvent],
    fired: &HashSet<(String, u64)>,
    now_epoch_secs: u64,
) -> Vec<UpcomingEvent> {
    events
        .iter()
        .filter(|e| e.start_epoch_secs <= now_epoch_secs + WARM_LEAD_SECS)
        .filter(|e| !fired.contains(&fired_key(e)))
        .cloned()
        .collect()
}

/// The maximum a scheduled sleep will ever last before we re-read the calendar,
/// even if the next meeting is far off. This is the SAFETY POLL: it caps the
/// sleep so a newly-added / moved meeting (that a sync missed) is still noticed
/// within this bound. 5 minutes — cheap, and far leaner than the old 30s scan.
pub const SAFETY_POLL_SECS: u64 = 300;

/// Pure scheduler core: how many seconds to sleep before the next action.
///
/// Returns the delay until the SOONEST not-yet-fired event reaches its warm
/// moment (`start - WARM_LEAD_SECS`), clamped to `[0, SAFETY_POLL_SECS]`. A
/// past-due warm moment returns 0 (fire now). No upcoming events → the full
/// safety-poll interval (just re-check the calendar later). This replaces the
/// fixed 30s busy-poll: the daemon sleeps precisely to the next meeting instead
/// of waking every 30s. Free of IO/time so it is unit-testable.
pub fn next_wake_secs(
    events: &[UpcomingEvent],
    fired: &HashSet<(String, u64)>,
    now_epoch_secs: u64,
) -> u64 {
    let soonest_warm_at = events
        .iter()
        .filter(|e| !fired.contains(&fired_key(e)))
        // The warm moment; saturating so a meeting already inside the lead window
        // maps to "now" (delay 0) rather than underflowing.
        .map(|e| e.start_epoch_secs.saturating_sub(WARM_LEAD_SECS))
        .min();

    match soonest_warm_at {
        Some(warm_at) => warm_at.saturating_sub(now_epoch_secs).min(SAFETY_POLL_SECS),
        None => SAFETY_POLL_SECS,
    }
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Poll interval helper (env `BLUEY_CALENDAR_POLL_SECS` for tests, min 1).
pub fn poll_interval() -> Duration {
    let secs = std::env::var("BLUEY_CALENDAR_POLL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|n| n.max(1))
        .unwrap_or(POLL_SECS);
    Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, start: u64) -> UpcomingEvent {
        UpcomingEvent {
            id: id.to_string(),
            title: id.to_string(),
            start_epoch_secs: start,
            participants: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn fires_once_per_occurrence_and_retries_until_success() {
        let mut fired = HashSet::new();
        let now = 1_000_000;

        // Outside the lead window → not yet.
        let events = vec![event("standup", now + WARM_LEAD_SECS + 60)];
        assert!(due_for_warmup(&events, &fired, now).is_empty());

        // Inside the lead → due. NOT consumed until the caller marks success:
        // a refused open (no agent attached yet) keeps retrying.
        let events = vec![event("standup", now + WARM_LEAD_SECS - 10)];
        assert_eq!(due_for_warmup(&events, &fired, now).len(), 1);
        assert_eq!(
            due_for_warmup(&events, &fired, now + 10).len(),
            1,
            "refused opens must retry"
        );

        // Success consumes the occurrence key → never double-fires.
        fired.insert(fired_key(&events[0]));
        assert!(due_for_warmup(&events, &fired, now + 20).is_empty());
    }

    #[test]
    fn moved_event_rearms_and_back_to_back_both_fire() {
        let mut fired = HashSet::new();
        let now = 2_000_000;

        // Fire + succeed at the original slot.
        let original = vec![event("planning", now + 60)];
        assert_eq!(due_for_warmup(&original, &fired, now).len(), 1);
        fired.insert(fired_key(&original[0]));

        // MOVED: same id, new occurrence start → new key → re-arms.
        let moved = vec![event("planning", now + 400)];
        assert!(
            due_for_warmup(&moved, &fired, now).is_empty(),
            "outside lead"
        );
        assert_eq!(due_for_warmup(&moved, &fired, now + 300).len(), 1);

        // Back-to-back distinct meetings both fire.
        let pair = vec![event("a", now + 500), event("b", now + 520)];
        assert_eq!(due_for_warmup(&pair, &fired, now + 450).len(), 2);
    }

    #[test]
    fn env_fake_source_parses_and_windows() {
        std::env::set_var(
            "BLUEY_CALENDAR_FAKE_EVENTS",
            "Standup@1000200; Retro@2000000; bad-entry",
        );
        let events = EnvFakeSource.upcoming(1_000_000);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].title, "Standup");
        std::env::remove_var("BLUEY_CALENDAR_FAKE_EVENTS");
    }

    #[test]
    fn next_wake_sleeps_to_the_warm_moment_not_a_fixed_tick() {
        let fired = HashSet::new();
        let now = 1_000_000;

        // No events → the full safety poll (just re-check later).
        assert_eq!(next_wake_secs(&[], &fired, now), SAFETY_POLL_SECS);

        // One event far out → sleep is CAPPED at the safety poll (not the raw
        // distance), so a later-added meeting is still noticed.
        let far = vec![event("x", now + 10_000)];
        assert_eq!(next_wake_secs(&far, &fired, now), SAFETY_POLL_SECS);

        // Event whose warm moment is 120s away (start-lead) → sleep exactly 120s.
        let soon = vec![event("y", now + WARM_LEAD_SECS + 120)];
        assert_eq!(next_wake_secs(&soon, &fired, now), 120);

        // Already inside the lead window → wake now (0).
        let due = vec![event("z", now + WARM_LEAD_SECS - 10)];
        assert_eq!(next_wake_secs(&due, &fired, now), 0);

        // The soonest of several drives the sleep.
        let many = vec![
            event("a", now + WARM_LEAD_SECS + 200),
            event("b", now + WARM_LEAD_SECS + 40),
            event("c", now + WARM_LEAD_SECS + 90),
        ];
        assert_eq!(next_wake_secs(&many, &fired, now), 40);

        // A fired event is ignored → the NEXT unfired one drives the sleep.
        let mut fired2 = HashSet::new();
        fired2.insert(fired_key(&many[1])); // b (40s) consumed
        assert_eq!(next_wake_secs(&many, &fired2, now), 90); // now c
    }
}
