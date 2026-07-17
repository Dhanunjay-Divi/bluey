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

/// One meeting participant (invitee or organizer) from the calendar event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    /// Display name when the calendar provides one (else empty).
    pub name: String,
    /// Email address, parsed from the participant's `mailto:` URL (else empty).
    pub email: String,
    /// True for the meeting organizer.
    pub is_organizer: bool,
}

/// One upcoming meeting occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpcomingEvent {
    /// Stable event id (iCalUID / EventKit identifier / fake title).
    pub id: String,
    pub title: String,
    /// Occurrence start (epoch seconds) — part of the dedupe key so a MOVED
    /// event re-arms.
    pub start_epoch_secs: u64,
    /// The meeting's invitees + organizer, when the calendar exposes them
    /// (EventKit does; the env fake leaves it empty). Names + emails let Bluey
    /// map diarized speakers to real people and hand the agent the roster.
    pub participants: Vec<Participant>,
}

/// Where upcoming events come from (EventKit or the env fake).
pub trait CalendarSource: Send + 'static {
    /// Events starting within `[now, now + LOOKAHEAD_SECS]`. Fail-soft:
    /// permission denied / source errors return an empty list.
    fn upcoming(&self, now_epoch_secs: u64) -> Vec<UpcomingEvent>;
}

/// Real system-calendar source via EventKit (macOS, feature `calendar`). Reads
/// the user's connected accounts (Outlook / Google / iCloud) through the OS
/// Calendar — an Outlook meeting added on the Mac shows up here automatically.
/// Requires the user's TCC consent; denied / not-yet-granted / any error returns
/// an empty list (the trait's fail-soft contract — never crash, never loop).
///
/// Stateless by design: EventKit's `EKEventStore` is NOT `Send`/`Sync`, but the
/// `CalendarSource` trait is `Send + 'static` (it runs on a tokio task). So the
/// store is created fresh inside each `upcoming()` call (on that call's thread)
/// and never held across an await/thread boundary. Creating a store is cheap
/// relative to the poll cadence (30s).
#[cfg(all(feature = "calendar", target_os = "macos"))]
pub struct EventKitSource {
    /// Fire the one-time access request the first time we read (so the macOS
    /// permission prompt appears), tracked with an atomic so we don't re-request.
    requested: std::sync::atomic::AtomicBool,
}

#[cfg(all(feature = "calendar", target_os = "macos"))]
impl EventKitSource {
    pub fn new() -> Self {
        Self {
            requested: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[cfg(all(feature = "calendar", target_os = "macos"))]
impl Default for EventKitSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "calendar", target_os = "macos"))]
impl CalendarSource for EventKitSource {
    fn upcoming(&self, now: u64) -> Vec<UpcomingEvent> {
        use objc2::rc::autoreleasepool;
        use objc2_event_kit::{EKAuthorizationStatus, EKEntityType, EKEventStore};
        use objc2_foundation::NSDate;
        use std::sync::atomic::Ordering;

        autoreleasepool(|_| unsafe {
            // Store lives only within this call (not Send — see the type doc).
            let store = EKEventStore::new();

            // First read: fire the access request so the OS prompt appears. The
            // completion block is required by the API; we don't act on it — the
            // next poll re-checks the authorization status.
            if !self.requested.swap(true, Ordering::Relaxed) {
                let completion = block2::RcBlock::new(
                    |_granted: objc2::runtime::Bool, _err: *mut objc2_foundation::NSError| {},
                );
                // The API wants a raw `*mut Block`; RcBlock derefs to Block.
                store.requestFullAccessToEventsWithCompletion(&*completion as *const _ as *mut _);
            }

            // Fail-soft until the user grants full access.
            if EKEventStore::authorizationStatusForEntityType(EKEntityType::Event)
                != EKAuthorizationStatus::FullAccess
            {
                return Vec::new();
            }

            let start = NSDate::dateWithTimeIntervalSince1970(now as f64);
            let end = NSDate::dateWithTimeIntervalSince1970((now + LOOKAHEAD_SECS) as f64);
            // `None` calendars = all of them (every connected account).
            let predicate =
                store.predicateForEventsWithStartDate_endDate_calendars(&start, &end, None);
            let events = store.eventsMatchingPredicate(&predicate);
            // Index-based iteration avoids extra objc2-foundation features.
            let mut out = Vec::new();
            for i in 0..events.count() {
                let ev = events.objectAtIndex(i);
                let start_secs = ev.startDate().timeIntervalSince1970();
                if !start_secs.is_finite() || start_secs < 0.0 {
                    continue;
                }
                // Stable id: the event identifier when present, else the title
                // (dedupe is keyed by (id, occurrence_start), so a moved event
                // still re-arms via the changed start).
                let title = ev.title().to_string();
                let id = ev
                    .eventIdentifier()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("ek-{title}"));

                // Participants: attendees + the organizer (names + emails), so
                // Bluey can map speakers to real people and give the agent the
                // roster. Any provider (Outlook/Google/iCloud) fills these.
                let mut participants = Vec::new();
                let organizer_email = ev.organizer().and_then(|o| participant_email(&o));
                if let Some(attendees) = ev.attendees() {
                    for j in 0..attendees.count() {
                        let p = attendees.objectAtIndex(j);
                        let name = p.name().map(|s| s.to_string()).unwrap_or_default();
                        let email = participant_email(&p).unwrap_or_default();
                        let is_organizer =
                            !email.is_empty() && organizer_email.as_deref() == Some(email.as_str());
                        participants.push(Participant {
                            name,
                            email,
                            is_organizer,
                        });
                    }
                }

                out.push(UpcomingEvent {
                    id,
                    title,
                    start_epoch_secs: start_secs as u64,
                    participants,
                });
            }
            out
        })
    }
}

/// Parse a participant's email from its EventKit `URL` (a `mailto:` NSURL).
/// Returns None when the URL isn't a mailto or is absent.
#[cfg(all(feature = "calendar", target_os = "macos"))]
fn participant_email(p: &objc2_event_kit::EKParticipant) -> Option<String> {
    // SAFETY: reading the participant's own URL + its string form.
    unsafe {
        let url = p.URL();
        let s = url.absoluteString()?.to_string();
        s.strip_prefix("mailto:").map(|e| e.to_string())
    }
}

/// A source that never yields events — the fallback when neither the env fake
/// nor a real calendar backend is available.
pub struct NoopSource;

impl CalendarSource for NoopSource {
    fn upcoming(&self, _now: u64) -> Vec<UpcomingEvent> {
        Vec::new()
    }
}

/// Pick the calendar source the daemon should poll: the env fake when
/// `BLUEY_CALENDAR_FAKE_EVENTS` is set (the deterministic test hook wins so a
/// test never races the real calendar), else the real EventKit source when the
/// `calendar` feature is compiled on macOS, else a no-op (the trigger stays
/// dormant rather than erroring).
pub fn default_source() -> Box<dyn CalendarSource> {
    if std::env::var("BLUEY_CALENDAR_FAKE_EVENTS").is_ok() {
        return Box::new(EnvFakeSource);
    }
    #[cfg(all(feature = "calendar", target_os = "macos"))]
    {
        return Box::new(EventKitSource::new());
    }
    #[allow(unreachable_code)]
    Box::new(NoopSource)
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
            participants: Vec::new(),
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
