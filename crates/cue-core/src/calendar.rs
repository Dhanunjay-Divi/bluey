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
/// (which provider, whether this build is configured, whether credentials are
/// currently usable, and the connected account email for the label). Tokens
/// never appear here; they live in the OS keychain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarConnection {
    /// Provider id: `"google"` or `"microsoft"`.
    pub provider: String,
    /// True when the current runtime/build configuration has a valid public
    /// OAuth client ID.
    #[serde(default)]
    pub configured: bool,
    /// True when credentials exist and can supply a live access token.
    pub connected: bool,
    /// The connected account's email for the UI label (empty when unknown or
    /// not connected).
    pub email: String,
    /// Sanitized configuration/credential health error, when action is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// An invitee's RSVP to the meeting, when the calendar exposes it. Maps Google's
/// `attendee.responseStatus` and Microsoft's `attendee.status.response` onto one
/// enum so pre-context can say who's actually coming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseStatus {
    /// No response recorded / provider didn't expose one.
    #[default]
    Unknown,
    Accepted,
    Declined,
    Tentative,
    /// Invited but not yet responded (Google `needsAction`, MS `notResponded`).
    NeedsAction,
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
    /// The invitee's RSVP, for pre-context ("3 of 5 accepted"). Defaults to
    /// `Unknown` for sources that don't expose it (EventKit fake / older paths).
    pub response: ResponseStatus,
}

/// Provider owning a calendar occurrence and its raw event identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CalendarProvider {
    #[default]
    Unknown,
    Google,
    Microsoft,
    Fake,
}

impl CalendarProvider {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Google => "google",
            Self::Microsoft => "microsoft",
            Self::Fake => "fake",
        }
    }
}

/// One upcoming meeting occurrence. Carries as much PRE-CONTEXT as the source
/// exposes — title, agenda, location, join URL, roster — so Bluey can brief the
/// agent before the meeting starts. All fields beyond the original core four
/// default to empty so existing sources compile unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpcomingEvent {
    /// Bluey's stable, globally namespaced occurrence identity. The dynamic
    /// aggregate source prefixes the raw provider id (for example
    /// `google:<id>`) so two connected providers cannot collide. This remains
    /// the internal dedupe/UI-response key and must never be passed to a provider
    /// connector as if it were the provider's own id.
    pub id: String,
    /// Calendar provider that owns [`Self::provider_event_id`].
    pub provider: CalendarProvider,
    /// Exact provider event/occurrence id, without Bluey's namespace prefix.
    /// Google connectors require the Google event id; Graph connectors require
    /// the immutable Graph id. Keep this distinct from both [`Self::id`] and the
    /// conferencing [`Self::meeting_id`].
    pub provider_event_id: String,
    /// Conferencing meeting id/code exposed by the provider (Google Meet
    /// `conferenceData.conferenceId` / `entryPoints[].meetingCode`, Microsoft
    /// Teams `onlineMeeting.conferenceId`). Empty when unavailable; callers
    /// must not substitute the provider event id or parse an unstable join URL.
    /// This is internal correlation metadata, not a user-facing label.
    pub meeting_id: String,
    pub title: String,
    /// Occurrence start (epoch seconds) — part of the dedupe key so a MOVED
    /// event re-arms.
    pub start_epoch_secs: u64,
    /// Occurrence end (epoch seconds), when the source exposes it. `0` = unknown.
    /// Lets pre-context state the meeting's length.
    pub end_epoch_secs: u64,
    /// The meeting's invitees + organizer, when the calendar exposes them
    /// (EventKit does; the env fake leaves it empty). Names + emails let Bluey
    /// map diarized speakers to real people and hand the agent the roster.
    pub participants: Vec<Participant>,
    /// The event body / agenda / notes (Google `description`, MS `body`/
    /// `bodyPreview`). The richest pre-context signal. Empty when absent.
    pub description: String,
    /// Physical or free-text location (Google `location`, MS `location`). Empty
    /// when absent.
    pub location: String,
    /// The video-call join URL (Google Meet `hangoutLink`/`conferenceData`, MS
    /// Teams `onlineMeeting.joinUrl`), or a URL parsed from the location/body
    /// (Zoom links, etc.). Empty when the meeting has no online component.
    pub join_url: String,
    /// The organizer's display name, when known (pre-context: "organized by …").
    pub organizer_name: String,
    /// The organizer's email, when known.
    pub organizer_email: String,
}

/// Where upcoming events come from (EventKit, the env fake, or a cloud
/// OAuth calendar). Implementors run on the daemon's poll task.
pub trait CalendarSource: Send + 'static {
    /// Events starting within `[now, now + LOOKAHEAD_SECS]`. Fail-soft:
    /// permission denied / source errors return an empty list.
    fn upcoming(&self, now_epoch_secs: u64) -> Vec<UpcomingEvent>;
}
