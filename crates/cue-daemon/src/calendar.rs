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

/// Fire the warm drive this many seconds before the event starts.
pub const WARM_LEAD_SECS: u64 = 180;
/// Rolling look-ahead window the poll scans.
pub const LOOKAHEAD_SECS: u64 = 600;
/// Poll cadence.
pub const POLL_SECS: u64 = 30;

/// One upcoming meeting occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpcomingEvent {
    /// Stable event id (iCalUID / EventKit identifier / fake title).
    pub id: String,
    pub title: String,
    /// Occurrence start (epoch seconds) — part of the dedupe key so a MOVED
    /// event re-arms.
    pub start_epoch_secs: u64,
}

/// Where upcoming events come from (EventKit or the env fake).
pub trait CalendarSource: Send + 'static {
    /// Events starting within `[now, now + LOOKAHEAD_SECS]`. Fail-soft:
    /// permission denied / source errors return an empty list.
    fn upcoming(&self, now_epoch_secs: u64) -> Vec<UpcomingEvent>;
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
}
