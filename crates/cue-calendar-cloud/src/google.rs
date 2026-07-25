//! Google Calendar client + [`CalendarSource`] impl.
//!
//! Three pieces:
//! 1. [`fetch_events`] — `calendar/v3/.../events` → `Vec<UpcomingEvent>`,
//!    fail-soft per item (a bad row is skipped, not the batch).
//! 2. [`fetch_email`] — `oauth2/v2/userinfo` → the connected account email, used
//!    to enrich the empty `CalTokens.email` that [`connect_interactive`] returns.
//! 3. [`GoogleCalendarSource`] — a background-refreshed snapshot behind a
//!    `Mutex`. A tokio task refreshes every ~45s; [`CalendarSource::upcoming`]
//!    just clones the last good snapshot, so it stays sync + non-blocking and
//!    never calls `block_on` (honoring the daemon's "never block the poll").
//!
//! Date handling is hand-rolled (see [`epoch_to_rfc3339_utc`] /
//! [`parse_rfc3339_to_epoch`]): neither `chrono` nor `time` is a direct dep of
//! this crate or `cue-core`, and the spec forbids adding one, so RFC3339 UTC is
//! formatted/parsed from epoch seconds directly.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::provider::Provider;
use crate::tokens::{CalTokenStore, CalTokens, KeyringCalStore};
use crate::{connect_interactive, valid_access_token, CalendarSource, Participant, UpcomingEvent};

/// How far ahead we ask Google for events. The daemon's own EventKit source uses
/// `LOOKAHEAD_SECS = 600`; `cue-core` does not re-export that constant, so we
/// mirror the value here.
pub const LOOKAHEAD_SECS: u64 = 600;

/// Background refresh cadence for the snapshot task (~45s, within the spec's
/// 30–60s window and comfortably under the 30s daemon poll's tolerance).
const REFRESH_INTERVAL: Duration = Duration::from_secs(45);

const EVENTS_URL: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

// --- Wire types (the subset of Google's JSON we read) --------------------

#[derive(Debug, Deserialize)]
struct EventsResponse {
    #[serde(default)]
    items: Vec<RawEvent>,
    #[serde(rename = "nextSyncToken", default)]
    next_sync_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "iCalUID", default)]
    ical_uid: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    /// The agenda/notes body — the richest pre-context signal.
    #[serde(default)]
    description: Option<String>,
    /// Free-text location (a room, an address, or a bare join link).
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    start: Option<EventDateTime>,
    #[serde(default)]
    end: Option<EventDateTime>,
    #[serde(default)]
    organizer: Option<RawOrganizer>,
    #[serde(default)]
    attendees: Vec<RawAttendee>,
    /// Read-only Google Meet/Hangout link, when the event has one.
    #[serde(rename = "hangoutLink", default)]
    hangout_link: Option<String>,
    /// Structured conference data (Meet + third-party). We read its entry points
    /// for a video join URL when `hangoutLink` is absent.
    #[serde(rename = "conferenceData", default)]
    conference_data: Option<RawConferenceData>,
    /// Cancelled events (in an incremental sync) carry `status == "cancelled"`;
    /// we drop them so a deleted meeting stops triggering.
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EventDateTime {
    /// A timed event's RFC3339 start (e.g. `2026-07-17T09:30:00-07:00`). All-day
    /// events instead carry a date-only `date` field (which we don't read): the
    /// absence of `dateTime` is exactly what makes [`map_event`] skip them.
    #[serde(rename = "dateTime", default)]
    date_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawOrganizer {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawAttendee {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(default)]
    organizer: bool,
    /// `accepted` | `declined` | `tentative` | `needsAction`.
    #[serde(rename = "responseStatus", default)]
    response_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawConferenceData {
    #[serde(rename = "entryPoints", default)]
    entry_points: Vec<RawEntryPoint>,
}

#[derive(Debug, Deserialize)]
struct RawEntryPoint {
    /// `video` | `phone` | `sip` | `more`. We take the `video` URI.
    #[serde(rename = "entryPointType", default)]
    entry_point_type: Option<String>,
    #[serde(default)]
    uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(default)]
    email: Option<String>,
}

// --- Date helpers (hand-rolled RFC3339 UTC, no date crate) ---------------

/// Wall-clock now in epoch seconds.
fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether `year` is a Gregorian leap year.
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in `month` (1-12) of `year`.
fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Format an epoch-second instant as RFC3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// Used only for `timeMin` / `timeMax` query params, so a plain civil-time
/// decomposition from the Unix epoch is sufficient (no timezone math).
fn epoch_to_rfc3339_utc(epoch: u64) -> String {
    let mut days = (epoch / 86_400) as i64;
    let secs_of_day = (epoch % 86_400) as u32;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let mut year: i64 = 1970;
    loop {
        let year_days = if is_leap_year(year) { 366 } else { 365 };
        if days >= year_days {
            days -= year_days;
            year += 1;
        } else {
            break;
        }
    }
    let mut month: u32 = 1;
    loop {
        let dim = days_in_month(year, month) as i64;
        if days >= dim {
            days -= dim;
            month += 1;
        } else {
            break;
        }
    }
    let day = days + 1;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days from the Unix epoch (1970-01-01) to `year-month-day` (proleptic
/// Gregorian, `month` 1-12, `day` 1-31). Returns `None` on an out-of-range date.
fn days_from_epoch(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) {
        return None;
    }
    let dim = days_in_month(year, month);
    if day < 1 || day > dim {
        return None;
    }
    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    for m in 1..month {
        days += days_in_month(year, m) as i64;
    }
    days += (day - 1) as i64;
    Some(days)
}

/// Parse an RFC3339 timestamp into epoch seconds (UTC).
///
/// Handles the shapes Google emits for `start.dateTime`: a `Z` suffix or a
/// numeric `±HH:MM` offset, with optional fractional seconds. Returns `None` for
/// anything it cannot confidently parse (date-only strings included), so callers
/// skip that event rather than guessing.
fn parse_rfc3339_to_epoch(s: &str) -> Option<i64> {
    let s = s.trim();
    // Split date and time on the 'T' (RFC3339 allows lowercase 't' too).
    let (date_part, rest) = s.split_once('T').or_else(|| s.split_once('t'))?;

    let mut date_iter = date_part.split('-');
    let year: i64 = date_iter.next()?.parse().ok()?;
    let month: u32 = date_iter.next()?.parse().ok()?;
    let day: u32 = date_iter.next()?.parse().ok()?;
    if date_iter.next().is_some() {
        return None;
    }

    // Separate the wall-clock time from the trailing timezone designator.
    let (time_str, offset_secs) = split_offset(rest)?;

    let mut time_iter = time_str.split(':');
    let hour: i64 = time_iter.next()?.parse().ok()?;
    let minute: i64 = time_iter.next()?.parse().ok()?;
    // Seconds may carry a fractional part (e.g. "05.250"); truncate it.
    let sec_field = time_iter.next().unwrap_or("0");
    let sec_whole = sec_field.split('.').next().unwrap_or("0");
    let second: i64 = sec_whole.parse().ok()?;
    if time_iter.next().is_some() {
        return None;
    }
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=60).contains(&second) {
        return None;
    }

    let days = days_from_epoch(year, month, day)?;
    let wall = days
        .checked_mul(86_400)?
        .checked_add(hour * 3600 + minute * 60 + second)?;
    // The instant in UTC = wall-clock time minus the local offset.
    wall.checked_sub(offset_secs)
}

/// Split a `HH:MM:SS[.fff]<TZ>` tail into `(time_without_tz, offset_seconds)`.
/// `<TZ>` is `Z`/`z` (offset 0) or `±HH:MM`. Returns `None` if no valid
/// designator is present (RFC3339 requires one).
fn split_offset(rest: &str) -> Option<(&str, i64)> {
    if let Some(stripped) = rest.strip_suffix('Z').or_else(|| rest.strip_suffix('z')) {
        return Some((stripped, 0));
    }
    // Find the offset sign, but not the '-' inside a date (already split off) —
    // here `rest` is time-only, so the first +/- after position 0 is the offset.
    let bytes = rest.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'+' || b == b'-') && i > 0 {
            let (time, tz) = rest.split_at(i);
            let sign = if b == b'+' { 1 } else { -1 };
            let tz_body = &tz[1..];
            let mut tz_iter = tz_body.split(':');
            let oh: i64 = tz_iter.next()?.parse().ok()?;
            let om: i64 = tz_iter.next().unwrap_or("0").parse().ok()?;
            if tz_iter.next().is_some() || !(0..=23).contains(&oh) || !(0..=59).contains(&om) {
                return None;
            }
            return Some((time, sign * (oh * 3600 + om * 60)));
        }
    }
    None
}

// --- Pure mapping: Google events JSON → Vec<UpcomingEvent> ---------------

/// Map one raw event to an [`UpcomingEvent`], or `None` if it should be skipped
/// (unparseable / all-day-only start). Fail-soft — never panics.
fn map_event(raw: RawEvent) -> Option<UpcomingEvent> {
    // Cancelled events (surfaced by incremental sync) are deletions — skip them
    // so a removed meeting stops arming the trigger.
    if raw.status.as_deref() == Some("cancelled") {
        return None;
    }

    // Start must be a parseable timed dateTime; all-day (`date`-only) and
    // unparseable events are skipped per the contract.
    let start = raw.start.as_ref()?;
    let date_time = start.date_time.as_deref()?;
    let start_epoch = parse_rfc3339_to_epoch(date_time)?;
    if start_epoch < 0 {
        return None;
    }
    let start_epoch_secs = start_epoch as u64;

    // End is best-effort (pre-context "how long is this"); unparseable = 0.
    let end_epoch_secs = raw
        .end
        .as_ref()
        .and_then(|e| e.date_time.as_deref())
        .and_then(parse_rfc3339_to_epoch)
        .filter(|&e| e >= 0)
        .map(|e| e as u64)
        .unwrap_or(0);

    // Prefer the stable iCalUID; fall back to the event id.
    let id = raw
        .ical_uid
        .filter(|s| !s.trim().is_empty())
        .or_else(|| raw.id.filter(|s| !s.trim().is_empty()))?;

    let title = raw.summary.unwrap_or_default();
    let description = raw.description.unwrap_or_default();
    let location = raw.location.clone().unwrap_or_default();

    // Join URL: prefer the Meet `hangoutLink`, else a `video` conference entry
    // point, else a URL sniffed from the location text (Zoom/Teams pasted in).
    let join_url = raw
        .hangout_link
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            raw.conference_data.as_ref().and_then(|c| {
                c.entry_points
                    .iter()
                    .find(|e| e.entry_point_type.as_deref() == Some("video"))
                    .and_then(|e| e.uri.clone())
            })
        })
        .or_else(|| raw.location.as_deref().and_then(sniff_url))
        .unwrap_or_default();

    let mut participants: Vec<Participant> = raw
        .attendees
        .into_iter()
        .map(|a| Participant {
            name: a.display_name.unwrap_or_default(),
            email: a.email.unwrap_or_default(),
            is_organizer: a.organizer,
            response: map_google_response(a.response_status.as_deref()),
        })
        .collect();

    // Fold in the top-level organizer: mark the matching attendee, or add one.
    let mut organizer_name = String::new();
    let mut organizer_email = String::new();
    if let Some(org) = raw.organizer {
        organizer_email = org.email.unwrap_or_default();
        organizer_name = org.display_name.unwrap_or_default();
        let matched = participants
            .iter_mut()
            .find(|p| !organizer_email.is_empty() && p.email.eq_ignore_ascii_case(&organizer_email));
        match matched {
            Some(p) => {
                p.is_organizer = true;
                if p.name.is_empty() && !organizer_name.is_empty() {
                    p.name = organizer_name.clone();
                }
            }
            None if !organizer_email.is_empty() || !organizer_name.is_empty() => {
                participants.push(Participant {
                    name: organizer_name.clone(),
                    email: organizer_email.clone(),
                    is_organizer: true,
                    response: cue_core::calendar::ResponseStatus::Accepted,
                });
            }
            None => {}
        }
    }

    Some(UpcomingEvent {
        id,
        title,
        start_epoch_secs,
        end_epoch_secs,
        participants,
        description,
        location,
        join_url,
        organizer_name,
        organizer_email,
    })
}

/// Map Google's `attendee.responseStatus` string onto [`ResponseStatus`].
fn map_google_response(s: Option<&str>) -> cue_core::calendar::ResponseStatus {
    use cue_core::calendar::ResponseStatus as R;
    match s {
        Some("accepted") => R::Accepted,
        Some("declined") => R::Declined,
        Some("tentative") => R::Tentative,
        Some("needsAction") => R::NeedsAction,
        _ => R::Unknown,
    }
}

/// Best-effort: pull the first http(s) URL out of free text (a join link pasted
/// into the location or body). Returns `None` when there's no URL. Shared with
/// the Microsoft source.
pub(crate) fn sniff_url(text: &str) -> Option<String> {
    let start = text.find("http")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"')
        .unwrap_or(rest.len());
    let url = &rest[..end];
    (url.starts_with("http://") || url.starts_with("https://")).then(|| url.to_string())
}

/// Parse a Google `events.list` JSON body into `UpcomingEvent`s, skipping any
/// item that fails to map (pure — no I/O, unit-tested).
fn parse_events_json(body: &str) -> Result<Vec<UpcomingEvent>> {
    let parsed: EventsResponse =
        serde_json::from_str(body).context("parse Google events.list JSON")?;
    Ok(parsed.items.into_iter().filter_map(map_event).collect())
}

// --- HTTP: events + email -----------------------------------------------

/// Outcome of a Google `events.list` query.
#[derive(Debug)]
pub enum FetchEventsOutcome {
    Success {
        events: Vec<UpcomingEvent>,
        next_sync_token: Option<String>,
    },
    /// The `syncToken` expired (HTTP 410 GONE); caller must trigger a full resync.
    TokenGone,
}

/// Fetch events from Google, supporting both full query and incremental `syncToken` query.
pub async fn fetch_events_with_sync(
    access_token: &str,
    now_epoch: u64,
    lookahead_secs: u64,
    sync_token: Option<&str>,
) -> Result<FetchEventsOutcome> {
    let client = reqwest::Client::new();
    let mut req = client.get(EVENTS_URL).bearer_auth(access_token);

    if let Some(token) = sync_token {
        req = req.query(&[("syncToken", token), ("maxResults", "100")]);
    } else {
        let time_min = epoch_to_rfc3339_utc(now_epoch);
        let time_max = epoch_to_rfc3339_utc(now_epoch.saturating_add(lookahead_secs));
        req = req.query(&[
            ("timeMin", time_min.as_str()),
            ("timeMax", time_max.as_str()),
            ("singleEvents", "true"),
            ("orderBy", "startTime"),
            ("maxResults", "50"),
        ]);
    }

    let resp = req.send().await.context("GET Google calendar events")?;
    let status = resp.status();
    if status == reqwest::StatusCode::GONE {
        return Ok(FetchEventsOutcome::TokenGone);
    }

    let body = resp
        .text()
        .await
        .context("read Google events response body")?;
    if !status.is_success() {
        return Err(anyhow!("Google events endpoint returned {status}: {body}"));
    }

    let parsed: EventsResponse =
        serde_json::from_str(&body).context("parse Google events.list JSON")?;
    let next_sync_token = parsed.next_sync_token;
    let events = parsed.items.into_iter().filter_map(map_event).collect();

    Ok(FetchEventsOutcome::Success {
        events,
        next_sync_token,
    })
}

/// Fetch upcoming events from the primary calendar as `UpcomingEvent`s.
pub async fn fetch_events(
    access_token: &str,
    now_epoch: u64,
    lookahead_secs: u64,
) -> Result<Vec<UpcomingEvent>> {
    match fetch_events_with_sync(access_token, now_epoch, lookahead_secs, None).await? {
        FetchEventsOutcome::Success { events, .. } => Ok(events),
        FetchEventsOutcome::TokenGone => Ok(Vec::new()),
    }
}

/// Fetch the connected account's email via the OAuth2 userinfo endpoint.
///
/// Best-effort: any failure (network, non-2xx, missing field) yields an empty
/// string so a connect never fails just because the label lookup did.
pub async fn fetch_email(access_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = match client
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return Ok(String::new()),
    };
    if !resp.status().is_success() {
        return Ok(String::new());
    }
    let info: UserInfo = match resp.json().await {
        Ok(i) => i,
        Err(_) => return Ok(String::new()),
    };
    Ok(info.email.unwrap_or_default())
}

// --- CalendarSource: background-refreshed snapshot -----------------------

/// A [`CalendarSource`] backed by Google Calendar.
///
/// A background task (spawned on the daemon's tokio handle) refreshes the event
/// snapshot every ~45s; [`CalendarSource::upcoming`] returns a clone of the last
/// good snapshot. On any refresh error the previous snapshot is kept (fail-soft),
/// so a transient network blip never empties the calendar mid-meeting.
pub struct GoogleCalendarSource {
    snapshot: Arc<Mutex<Vec<UpcomingEvent>>>,
}

impl GoogleCalendarSource {
    /// Spawn the background refresh task on `handle` and return the source.
    ///
    /// The task loops forever: `valid_access_token` → `fetch_events` → store.
    /// It holds only a `Weak`-free `Arc` clone of the snapshot and the store, so
    /// it lives as long as the process (matching the daemon's other sources).
    pub fn spawn(store: Arc<dyn CalTokenStore>, handle: tokio::runtime::Handle) -> Self {
        let snapshot: Arc<Mutex<Vec<UpcomingEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let task_snapshot = Arc::clone(&snapshot);
        let cfg = Provider::Google.config();

        handle.spawn(async move {
            let mut ticker = tokio::time::interval(REFRESH_INTERVAL);
            let mut sync_token: Option<String> = None;

            loop {
                ticker.tick().await;
                let now = now_epoch_secs();
                match valid_access_token(store.as_ref(), &cfg, now).await {
                    Ok(token) => {
                        match fetch_events_with_sync(&token, now, LOOKAHEAD_SECS, sync_token.as_deref()).await {
                            Ok(FetchEventsOutcome::Success { events, next_sync_token }) => {
                                if let Some(nst) = next_sync_token {
                                    sync_token = Some(nst);
                                }
                                match task_snapshot.lock() {
                                    Ok(mut guard) => *guard = events,
                                    Err(e) => tracing::warn!(
                                        error = %e,
                                        "google calendar snapshot mutex poisoned; keeping last snapshot"
                                    ),
                                }
                            }
                            Ok(FetchEventsOutcome::TokenGone) => {
                                tracing::info!("Google calendar syncToken expired (410 GONE) — performing baseline resync");
                                sync_token = None;
                                if let Ok(FetchEventsOutcome::Success { events, next_sync_token }) =
                                    fetch_events_with_sync(&token, now, LOOKAHEAD_SECS, None).await
                                {
                                    sync_token = next_sync_token;
                                    if let Ok(mut guard) = task_snapshot.lock() {
                                        *guard = events;
                                    }
                                }
                            }
                            Err(e) => tracing::warn!(
                                error = %e,
                                "google calendar events fetch failed; keeping last snapshot"
                            ),
                        }
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "google calendar token unavailable; keeping last snapshot"
                    ),
                }
            }
        });

        Self { snapshot }
    }

    /// Connect a Google account interactively, enrich the email label, persist
    /// the tokens to the per-provider keyring, and return them.
    ///
    /// `open_browser` is injected by the caller (the daemon shells out to
    /// `open`/`xdg-open`/`start`). D calls this from the IPC connect handler.
    pub async fn connect(open_browser: impl Fn(&str)) -> Result<CalTokens> {
        let mut tokens = connect_interactive(Provider::Google, open_browser).await?;
        // Best-effort email enrichment for the connected-account UI label.
        if tokens.email.trim().is_empty() {
            if let Ok(email) = fetch_email(&tokens.access).await {
                tokens.email = email;
            }
        }
        let store = KeyringCalStore::new(Provider::Google.keyring_service());
        store
            .save(&tokens)
            .context("persist Google calendar tokens to keyring")?;
        Ok(tokens)
    }
}

impl CalendarSource for GoogleCalendarSource {
    fn upcoming(&self, _now_epoch_secs: u64) -> Vec<UpcomingEvent> {
        // Non-blocking, sync: just clone the last good snapshot. A poisoned lock
        // is treated as "no events" (fail-soft) rather than panicking.
        match self.snapshot.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_to_rfc3339_utc_matches_known_instants() {
        // 2021-01-01T00:00:00Z = 1609459200
        assert_eq!(epoch_to_rfc3339_utc(1_609_459_200), "2021-01-01T00:00:00Z");
        // Unix epoch itself.
        assert_eq!(epoch_to_rfc3339_utc(0), "1970-01-01T00:00:00Z");
        // A leap-day instant: 2024-02-29T12:34:56Z = 1709210096
        assert_eq!(epoch_to_rfc3339_utc(1_709_210_096), "2024-02-29T12:34:56Z");
    }

    #[test]
    fn parse_rfc3339_round_trips_utc() {
        let epoch = 1_609_459_200;
        let s = epoch_to_rfc3339_utc(epoch);
        assert_eq!(parse_rfc3339_to_epoch(&s), Some(epoch as i64));
    }

    #[test]
    fn parse_rfc3339_applies_offset() {
        // 09:30:00-07:00 is 16:30:00Z.
        let with_offset = parse_rfc3339_to_epoch("2026-07-17T09:30:00-07:00").unwrap();
        let as_utc = parse_rfc3339_to_epoch("2026-07-17T16:30:00Z").unwrap();
        assert_eq!(with_offset, as_utc);
        // A positive offset shifts the UTC instant the other way.
        let plus = parse_rfc3339_to_epoch("2026-07-17T09:30:00+02:00").unwrap();
        let plus_utc = parse_rfc3339_to_epoch("2026-07-17T07:30:00Z").unwrap();
        assert_eq!(plus, plus_utc);
    }

    #[test]
    fn parse_rfc3339_handles_fractional_seconds() {
        let frac = parse_rfc3339_to_epoch("2026-07-17T16:30:00.250Z").unwrap();
        let plain = parse_rfc3339_to_epoch("2026-07-17T16:30:00Z").unwrap();
        assert_eq!(frac, plain);
    }

    #[test]
    fn parse_rfc3339_rejects_date_only() {
        assert_eq!(parse_rfc3339_to_epoch("2026-07-17"), None);
        assert_eq!(parse_rfc3339_to_epoch("not-a-date"), None);
    }

    /// A realistic Google `events.list` sample: one fully-populated timed event
    /// with attendees + a top-level organizer NOT in the attendee list, one
    /// all-day event that must be skipped, and one event whose organizer IS an
    /// attendee (to exercise the mark-in-place path).
    const SAMPLE: &str = r#"{
      "kind": "calendar#events",
      "items": [
        {
          "id": "evt-1",
          "iCalUID": "ical-uid-1@google.com",
          "summary": "Design Review",
          "start": { "dateTime": "2026-07-17T16:30:00Z" },
          "organizer": { "email": "boss@example.com", "displayName": "The Boss" },
          "attendees": [
            { "email": "alice@example.com", "displayName": "Alice", "organizer": false },
            { "email": "bob@example.com", "displayName": "Bob", "responseStatus": "accepted" }
          ]
        },
        {
          "id": "evt-allday",
          "summary": "Company Holiday",
          "start": { "date": "2026-07-18" },
          "attendees": []
        },
        {
          "id": "evt-2",
          "summary": "Standup",
          "start": { "dateTime": "2026-07-17T09:00:00-07:00" },
          "organizer": { "email": "alice@example.com", "displayName": "Alice A." },
          "attendees": [
            { "email": "ALICE@example.com", "displayName": "Alice", "organizer": true },
            { "email": "carol@example.com", "displayName": "Carol" }
          ]
        }
      ]
    }"#;

    #[test]
    fn parses_sample_and_skips_all_day() {
        let events = parse_events_json(SAMPLE).unwrap();
        // The all-day event is skipped; two timed events remain.
        assert_eq!(events.len(), 2);

        // Event 1: iCalUID wins over id; organizer added as a participant.
        let e1 = &events[0];
        assert_eq!(e1.id, "ical-uid-1@google.com");
        assert_eq!(e1.title, "Design Review");
        assert_eq!(e1.start_epoch_secs, 1_784_305_800); // 2026-07-17T16:30:00Z
                                                        // 2 attendees + the organizer (not previously present) = 3.
        assert_eq!(e1.participants.len(), 3);
        let organizer = e1
            .participants
            .iter()
            .find(|p| p.is_organizer)
            .expect("organizer present");
        assert_eq!(organizer.email, "boss@example.com");
        assert_eq!(organizer.name, "The Boss");
        // Non-organizer attendees stay non-organizer.
        assert!(e1
            .participants
            .iter()
            .any(|p| p.email == "alice@example.com" && !p.is_organizer));
    }

    #[test]
    fn organizer_matched_in_place_not_duplicated() {
        let events = parse_events_json(SAMPLE).unwrap();
        let e2 = &events[1];
        assert_eq!(e2.id, "evt-2");
        // 09:00:00-07:00 == 16:00:00Z on 2026-07-17.
        assert_eq!(e2.start_epoch_secs, 1_784_304_000);
        // Organizer email matches an existing attendee (case-insensitively), so
        // no duplicate is added: still 2 participants.
        assert_eq!(e2.participants.len(), 2);
        let organizers: Vec<_> = e2.participants.iter().filter(|p| p.is_organizer).collect();
        assert_eq!(organizers.len(), 1);
        assert!(organizers[0]
            .email
            .eq_ignore_ascii_case("alice@example.com"));
    }

    #[test]
    fn empty_items_yields_empty_vec() {
        let events = parse_events_json(r#"{"items":[]}"#).unwrap();
        assert!(events.is_empty());
        // Missing items array entirely (serde default) also yields empty.
        let events2 = parse_events_json(r#"{"kind":"calendar#events"}"#).unwrap();
        assert!(events2.is_empty());
    }

    #[test]
    fn bad_item_is_skipped_not_fatal() {
        // One event with an unparseable dateTime, one good — only the good survives.
        let json = r#"{"items":[
          {"id":"bad","start":{"dateTime":"garbage"}},
          {"id":"good","summary":"OK","start":{"dateTime":"2026-07-17T16:30:00Z"}}
        ]}"#;
        let events = parse_events_json(json).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "good");
    }

    #[test]
    fn source_upcoming_clones_snapshot() {
        let snapshot = Arc::new(Mutex::new(vec![UpcomingEvent {
            id: "x".into(),
            title: "T".into(),
            start_epoch_secs: 123,
            participants: Vec::new(),
            ..Default::default()
        }]));
        let source = GoogleCalendarSource {
            snapshot: Arc::clone(&snapshot),
        };
        let got = source.upcoming(0);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "x");
    }
}
