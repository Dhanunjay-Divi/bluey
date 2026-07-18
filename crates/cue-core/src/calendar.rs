//! Shared calendar types — the pluggable-source seam for the warmup trigger.
//!
//! These types live in `cue-core` (not `cue-daemon`) so that BOTH the daemon's
//! EventKit/env-fake sources AND the out-of-daemon cloud calendar crate
//! (`cue-calendar-cloud`, OAuth Google/Microsoft) can implement the same
//! [`CalendarSource`] trait without a dependency cycle: the cloud crate depends
//! on `cue-core`, never on `cue-daemon`.
//!
//! The daemon's poll loop is agnostic to the concrete source — it only calls
//! [`CalendarSource::upcoming`]. See `cue-daemon/src/calendar.rs` for the
//! concrete `EventKitSource` / `EnvFakeSource` / `NoopSource` impls and the
//! dedupe / warm-fire trigger core.

use serde::{Deserialize, Serialize};

/// The connection state of ONE cloud-calendar provider, surfaced to the UI via
/// [`crate::ipc::DaemonResponse::CalendarStatus`]. A wire DTO (serde), unlike the
/// poll-source seam types below — it carries only non-secret connection metadata
/// (which provider, whether it is connected, and the connected account email for
/// the label). Tokens never appear here; they live in the OS keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarConnection {
    /// Provider id: `"google"` or `"microsoft"`.
    pub provider: String,
    /// True when tokens for this provider are stored on-device.
    pub connected: bool,
    /// The connected account's email for the UI label (empty when unknown or
    /// not connected).
    pub email: String,
}

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

/// Where upcoming events come from (EventKit, the env fake, or a cloud
/// OAuth calendar). Implementors run on the daemon's poll task.
pub trait CalendarSource: Send + 'static {
    /// Events starting within `[now, now + LOOKAHEAD_SECS]`. Fail-soft:
    /// permission denied / source errors return an empty list.
    fn upcoming(&self, now_epoch_secs: u64) -> Vec<UpcomingEvent>;
}
