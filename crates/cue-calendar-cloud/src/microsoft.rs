//! Microsoft Graph calendar client + [`CalendarSource`] implementation.
//!
//! Fetches upcoming events from Microsoft Graph's `calendarView` (v1.0) and
//! maps them to the shared [`UpcomingEvent`] type, plus a background-refreshed
//! [`MicrosoftCalendarSource`] so the daemon's sync `upcoming()` poll never
//! blocks on HTTP or a token refresh (the "background snapshot" strategy from
//! the build spec).
//!
//! ISO-8601 is hand-rolled (no `chrono`/`time` in the workspace dep set): a few
//! lines convert epoch↔UTC calendar for the `startDateTime`/`endDateTime` query
//! window and to parse Graph's `start.dateTime`. With the
//! `Prefer: outlook.timezone="UTC"` request header, Graph returns those times in
//! UTC, so we parse the wall-clock components as UTC directly. The same header
//! requests immutable event ids so moves between calendars do not change the
//! cache/dedupe identity.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::oauth::{connect_interactive, valid_access_token_serialized};
use crate::provider::Provider;
use crate::tokens::{CalTokenStore, CalTokens, KeyringCalStore};
use cue_core::calendar::{CalendarProvider, CalendarSource, Participant, UpcomingEvent};

/// Default lookahead window (seconds) for the calendar view: 10 minutes, enough
/// to arm the warmup trigger ahead of a meeting start.
pub const DEFAULT_LOOKAHEAD_SECS: u64 = 600;

/// How often the background task refreshes the cached snapshot.
const REFRESH_INTERVAL_SECS: u64 = 45;
/// Graph delta tokens are bound to their initial calendar-view range. Rebuild
/// the baseline periodically so the 10-minute lookahead continues to advance.
const BASELINE_REFRESH_SECS: u64 = 300;
const MAX_PAGES_PER_SYNC: usize = 100;

/// Microsoft Graph base (v1.0, NOT beta).
const GRAPH_CALENDAR_DELTA_URL: &str = "https://graph.microsoft.com/v1.0/me/calendarView/delta";
const GRAPH_ME_URL: &str = "https://graph.microsoft.com/v1.0/me";
const GRAPH_CALENDAR_PREFER: &str =
    "outlook.timezone=\"UTC\", IdType=\"ImmutableId\", odata.maxpagesize=100";

// --- Graph JSON shapes ---------------------------------------------------

#[derive(Debug, Deserialize)]
struct CalendarViewResponse {
    #[serde(default)]
    value: Vec<GraphEvent>,
    #[serde(rename = "@odata.nextLink", default)]
    next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink", default)]
    delta_link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    start: Option<GraphDateTime>,
    #[serde(default)]
    end: Option<GraphDateTime>,
    #[serde(default)]
    attendees: Vec<GraphAttendee>,
    #[serde(default)]
    organizer: Option<GraphRecipient>,
    /// Plain-text preview of the body (agenda) — pre-context without HTML.
    #[serde(rename = "bodyPreview", default)]
    body_preview: Option<String>,
    #[serde(default)]
    location: Option<GraphLocation>,
    /// The Teams join info, when `isOnlineMeeting` is true.
    #[serde(rename = "onlineMeeting", default)]
    online_meeting: Option<GraphOnlineMeeting>,
    /// Removed events (in a delta response) are tagged; we drop them.
    #[serde(rename = "@removed", default)]
    removed: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GraphDateTime {
    /// e.g. "2026-07-17T15:00:00.0000000" (UTC when the Prefer header is set).
    #[serde(rename = "dateTime")]
    #[serde(default)]
    date_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphLocation {
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphOnlineMeeting {
    #[serde(rename = "joinUrl", default)]
    join_url: Option<String>,
    /// The conference id exposed by Graph's `onlineMeetingInfo` event field.
    /// This is distinct from both the Graph event id and the unstable join URL.
    #[serde(rename = "conferenceId", default)]
    conference_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphAttendee {
    #[serde(rename = "emailAddress")]
    #[serde(default)]
    email_address: Option<GraphEmailAddress>,
    #[serde(default)]
    status: Option<GraphResponseStatus>,
}

#[derive(Debug, Deserialize)]
struct GraphResponseStatus {
    /// `none` | `accepted` | `declined` | `tentativelyAccepted` | `notResponded`
    /// | `organizer`.
    #[serde(default)]
    response: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphRecipient {
    #[serde(rename = "emailAddress")]
    #[serde(default)]
    email_address: Option<GraphEmailAddress>,
}

#[derive(Debug, Deserialize)]
struct GraphEmailAddress {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    address: Option<String>,
}

/// Subset of the Graph `/me` response we read for the connected-account label.
#[derive(Debug, Deserialize)]
struct GraphMe {
    #[serde(default)]
    mail: Option<String>,
    #[serde(rename = "userPrincipalName")]
    #[serde(default)]
    user_principal_name: Option<String>,
}

// --- ISO-8601 (hand-rolled; no chrono/time in the workspace dep set) ------

/// Days in each month of a (possibly leap) year, index 0 = January.
fn days_in_month(year: i64, month0: usize) -> i64 {
    const DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if month0 == 1 && is_leap_year(year) {
        29
    } else {
        DAYS[month0]
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Format an epoch-seconds instant as an ISO-8601 UTC string
/// (`YYYY-MM-DDTHH:MM:SSZ`) for the Graph query window params.
fn epoch_to_iso8601_utc(epoch_secs: u64) -> String {
    let mut days = (epoch_secs / 86_400) as i64;
    let rem = (epoch_secs % 86_400) as i64;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;

    // Walk forward from the Unix epoch (1970-01-01) counting whole years/months.
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
    let mut month0: usize = 0;
    loop {
        let dim = days_in_month(year, month0);
        if days >= dim {
            days -= dim;
            month0 += 1;
        } else {
            break;
        }
    }
    let day = days + 1; // day-of-month is 1-based
    let month = month0 as i64 + 1;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Parse an ISO-8601 date-time (as returned by Graph with the UTC Prefer header)
/// into epoch seconds, interpreting the wall-clock components as UTC.
///
/// Accepts forms like `2026-07-17T15:00:00`, `...:00.0000000`, `...:00Z`,
/// and `...:00+00:00`. A trailing non-UTC offset is honored. Returns `None` on
/// anything unparseable (caller fail-soft-skips the event).
fn iso8601_to_epoch(s: &str) -> Option<u64> {
    let s = s.trim();
    // Split date and time on the 'T' (or space) separator.
    let (date_part, rest) = {
        let bytes = s.as_bytes();
        let idx = bytes
            .iter()
            .position(|&b| b == b'T' || b == b't' || b == b' ')?;
        (&s[..idx], &s[idx + 1..])
    };

    let mut date_iter = date_part.split('-');
    let year: i64 = date_iter.next()?.parse().ok()?;
    let month: i64 = date_iter.next()?.parse().ok()?;
    let day: i64 = date_iter.next()?.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }

    // Separate the clock from any timezone designator (Z or ±HH:MM).
    let (clock, offset_secs) = split_offset(rest)?;
    let mut clock_iter = clock.split(':');
    let hour: i64 = clock_iter.next()?.parse().ok()?;
    let minute: i64 = clock_iter.next().unwrap_or("0").parse().ok()?;
    // Seconds may carry a fractional part ("00.0000000") — take the integer part.
    let sec_str = clock_iter.next().unwrap_or("0");
    let sec_int: i64 = sec_str.split('.').next().unwrap_or("0").parse().ok()?;

    // Days since the Unix epoch for this Y-M-D (proleptic Gregorian).
    let mut total_days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            total_days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            total_days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }
    for m0 in 0..(month as usize - 1) {
        total_days += days_in_month(year, m0);
    }
    total_days += day - 1;

    let secs = total_days * 86_400 + hour * 3600 + minute * 60 + sec_int - offset_secs;
    if secs < 0 {
        None
    } else {
        Some(secs as u64)
    }
}

/// Strip a trailing timezone designator from an ISO time, returning the bare
/// clock plus the offset (in seconds) to SUBTRACT to reach UTC.
/// `Z` / no designator → 0. `+HH:MM` → +offset, `-HH:MM` → -offset.
fn split_offset(clock: &str) -> Option<(&str, i64)> {
    if let Some(stripped) = clock.strip_suffix('Z').or_else(|| clock.strip_suffix('z')) {
        return Some((stripped, 0));
    }
    // Find a +/- that begins an offset. Skip index 0 so a leading sign (never
    // valid here) doesn't confuse the scan; offsets appear after the seconds.
    let bytes = clock.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (b == b'+' || b == b'-') {
            let (time, tz) = (&clock[..i], &clock[i..]);
            let sign = if tz.starts_with('-') { -1 } else { 1 };
            let digits = &tz[1..];
            let mut parts = digits.split(':');
            let oh: i64 = parts.next()?.parse().ok()?;
            let om: i64 = parts.next().unwrap_or("0").parse().ok()?;
            return Some((time, sign * (oh * 3600 + om * 60)));
        }
    }
    Some((clock, 0))
}

// --- Mapping Graph JSON → UpcomingEvent ----------------------------------

/// Map one Graph event to an [`UpcomingEvent`]. Returns `None` when the start
/// time is missing/unparseable (fail-soft: the caller skips it).
fn map_event(ev: GraphEvent) -> Option<UpcomingEvent> {
    // A delta response tags deletions with `@removed`; drop them so a cancelled
    // meeting stops arming the trigger.
    if ev.removed.is_some() {
        return None;
    }
    if ev.id.trim().is_empty() {
        return None;
    }
    let start_epoch_secs = ev
        .start
        .as_ref()
        .and_then(|d| d.date_time.as_deref())
        .and_then(iso8601_to_epoch)?;

    // Organizer email (lowercased) is the key to flag the organizer among attendees.
    let organizer_email = ev
        .organizer
        .as_ref()
        .and_then(|o| o.email_address.as_ref())
        .and_then(|e| e.address.as_deref())
        .map(|a| a.to_ascii_lowercase());

    let mut participants: Vec<Participant> = Vec::new();
    let mut organizer_seen = false;
    for att in ev.attendees {
        let email = att
            .email_address
            .as_ref()
            .and_then(|e| e.address.clone())
            .unwrap_or_default();
        let name = att
            .email_address
            .as_ref()
            .and_then(|e| e.name.clone())
            .unwrap_or_default();
        let is_organizer = organizer_email
            .as_deref()
            .is_some_and(|org| !email.is_empty() && email.eq_ignore_ascii_case(org));
        if is_organizer {
            organizer_seen = true;
        }
        let response = map_ms_response(att.status.as_ref().and_then(|s| s.response.as_deref()));
        participants.push(Participant {
            name,
            email,
            is_organizer,
            response,
        });
    }

    // Ensure the organizer is present in the roster even when Graph does not list
    // them among `attendees` (common: the organizer is not self-invited).
    let organizer_name = ev
        .organizer
        .as_ref()
        .and_then(|o| o.email_address.as_ref())
        .and_then(|e| e.name.clone())
        .unwrap_or_default();
    let organizer_email_str = ev
        .organizer
        .as_ref()
        .and_then(|o| o.email_address.as_ref())
        .and_then(|e| e.address.clone())
        .unwrap_or_default();
    if !organizer_seen && !organizer_email_str.is_empty() {
        participants.push(Participant {
            name: organizer_name.clone(),
            email: organizer_email_str.clone(),
            is_organizer: true,
            response: cue_core::calendar::ResponseStatus::Accepted,
        });
    }

    // End (best-effort) for pre-context duration.
    let end_epoch_secs = ev
        .end
        .as_ref()
        .and_then(|d| d.date_time.as_deref())
        .and_then(iso8601_to_epoch)
        .unwrap_or(0);

    let description = ev.body_preview.unwrap_or_default();
    let location = ev.location.and_then(|l| l.display_name).unwrap_or_default();
    let meeting_id = ev
        .online_meeting
        .as_ref()
        .and_then(|meeting| meeting.conference_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_default();
    // Join URL: the Teams `onlineMeeting.joinUrl`, else a URL sniffed from the
    // location text (a pasted Zoom/Meet link).
    let join_url = ev
        .online_meeting
        .as_ref()
        .and_then(|meeting| meeting.join_url.clone())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| super::google::sniff_url(&location))
        .unwrap_or_default();

    Some(UpcomingEvent {
        provider_event_id: ev.id.clone(),
        id: ev.id,
        provider: CalendarProvider::Microsoft,
        meeting_id,
        title: ev.subject.unwrap_or_default(),
        start_epoch_secs,
        end_epoch_secs,
        participants,
        description,
        location,
        join_url,
        organizer_name,
        organizer_email: organizer_email_str,
    })
}

/// Map Graph's `attendee.status.response` onto [`ResponseStatus`].
fn map_ms_response(s: Option<&str>) -> cue_core::calendar::ResponseStatus {
    use cue_core::calendar::ResponseStatus as R;
    match s {
        Some("accepted") | Some("organizer") => R::Accepted,
        Some("declined") => R::Declined,
        Some("tentativelyAccepted") => R::Tentative,
        Some("notResponded") => R::NeedsAction,
        _ => R::Unknown,
    }
}

/// Parse a full Graph `calendarView` JSON body into `UpcomingEvent`s, skipping
/// any item that fails to map (fail-soft per item).
#[cfg(test)]
fn parse_calendar_view(body: &str) -> Result<Vec<UpcomingEvent>> {
    let parsed: CalendarViewResponse =
        serde_json::from_str(body).context("parse Graph calendarView JSON")?;
    Ok(parsed.value.into_iter().filter_map(map_event).collect())
}

fn event_changes(events: Vec<GraphEvent>) -> (Vec<UpcomingEvent>, Vec<String>) {
    let mut upserts = Vec::new();
    let mut removed_ids = Vec::new();
    for event in events {
        if event.removed.is_some() {
            if !event.id.trim().is_empty() {
                removed_ids.push(event.id);
            }
            continue;
        }
        if let Some(event) = map_event(event) {
            upserts.push(event);
        }
    }
    (upserts, removed_ids)
}

fn apply_event_changes(
    cache: &mut HashMap<String, UpcomingEvent>,
    upserts: Vec<UpcomingEvent>,
    removed_ids: Vec<String>,
    replace: bool,
) {
    if replace {
        cache.clear();
    }
    for id in removed_ids {
        cache.remove(&id);
    }
    for event in upserts {
        cache.insert(event.id.clone(), event);
    }
}

fn snapshot_from_cache(
    cache: &HashMap<String, UpcomingEvent>,
    now_epoch: u64,
) -> Vec<UpcomingEvent> {
    let mut events = cache
        .values()
        .filter(|event| {
            event.start_epoch_secs >= now_epoch
                && event.start_epoch_secs <= now_epoch.saturating_add(DEFAULT_LOOKAHEAD_SECS)
        })
        .cloned()
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.start_epoch_secs);
    events
}

fn validate_graph_delta_url(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url).context("parse Microsoft Graph delta URL")?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("graph.microsoft.com") {
        return Err(anyhow!("Microsoft Graph returned an invalid delta URL"));
    }
    if parsed.path() != "/v1.0/me/calendarView/delta" {
        return Err(anyhow!(
            "Microsoft Graph delta URL used an unexpected resource path"
        ));
    }
    Ok(())
}

fn with_graph_calendar_preferences(request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    request.header("Prefer", GRAPH_CALENDAR_PREFER)
}

// --- HTTP calls ----------------------------------------------------------

/// Fetch upcoming events from Graph `calendarView` in `[now, now + lookahead]`.
///
/// Sets `Prefer: outlook.timezone="UTC", IdType="ImmutableId"` so
/// `start.dateTime` comes back in UTC and event ids stay stable across calendar
/// moves, and uses only the query parameters Graph permits on the initial delta
/// call. Fail-soft per item; a transport/HTTP error surfaces as `Err`.
/// Outcome of a Microsoft Graph calendar query.
#[derive(Debug)]
pub enum FetchMicrosoftEventsOutcome {
    Success {
        events: Vec<UpcomingEvent>,
        removed_ids: Vec<String>,
        delta_link: Option<String>,
    },
    /// The delta token expired (HTTP 410 GONE); caller must trigger a full resync.
    TokenGone,
}

/// Fetch events from Microsoft Graph, supporting both full query and `deltaLink` query.
pub async fn fetch_events_with_delta(
    access_token: &str,
    now_epoch: u64,
    lookahead_secs: u64,
    delta_url: Option<&str>,
) -> Result<FetchMicrosoftEventsOutcome> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build Microsoft Graph HTTP client")?;
    let start = epoch_to_iso8601_utc(now_epoch);
    let end = epoch_to_iso8601_utc(now_epoch.saturating_add(lookahead_secs));
    let mut next_url = delta_url.map(str::to_string);
    let mut events = Vec::new();
    let mut removed_ids = Vec::new();

    for _ in 0..MAX_PAGES_PER_SYNC {
        let request = if let Some(url) = next_url.as_deref() {
            validate_graph_delta_url(url)?;
            client.get(url)
        } else {
            client.get(GRAPH_CALENDAR_DELTA_URL).query(&[
                ("startDateTime", start.as_str()),
                ("endDateTime", end.as_str()),
            ])
        };
        let response = with_graph_calendar_preferences(request.bearer_auth(access_token))
            .send()
            .await
            .context("GET Microsoft Graph calendar delta")?;

        let status = response.status();
        if status == reqwest::StatusCode::GONE {
            return Ok(FetchMicrosoftEventsOutcome::TokenGone);
        }
        let body = response
            .text()
            .await
            .context("read Microsoft Graph response body")?;
        if !status.is_success() {
            return Err(anyhow!("Microsoft Graph Calendar returned HTTP {status}"));
        }

        let parsed: CalendarViewResponse =
            serde_json::from_str(&body).context("parse Graph calendar delta JSON")?;
        let (page_events, page_removed_ids) = event_changes(parsed.value);
        events.extend(page_events);
        removed_ids.extend(page_removed_ids);

        if let Some(url) = parsed.next_link {
            validate_graph_delta_url(&url)?;
            next_url = Some(url);
            continue;
        }
        if let Some(url) = parsed.delta_link.as_deref() {
            validate_graph_delta_url(url)?;
        }
        return Ok(FetchMicrosoftEventsOutcome::Success {
            events,
            removed_ids,
            delta_link: parsed.delta_link,
        });
    }

    Err(anyhow!(
        "Microsoft Graph pagination exceeded {MAX_PAGES_PER_SYNC} pages"
    ))
}

/// Fetch upcoming events from the primary calendar as `UpcomingEvent`s.
pub async fn fetch_events(
    access_token: &str,
    now_epoch: u64,
    lookahead_secs: u64,
) -> Result<Vec<UpcomingEvent>> {
    match fetch_events_with_delta(access_token, now_epoch, lookahead_secs, None).await? {
        FetchMicrosoftEventsOutcome::Success { events, .. } => Ok(events),
        FetchMicrosoftEventsOutcome::TokenGone => Ok(Vec::new()),
    }
}

/// Best-effort fetch of the connected account's email from Graph `/me`.
///
/// Reads `mail`, falling back to `userPrincipalName`. Any error yields an empty
/// string (the email is only a UI label), so callers can `unwrap_or_default`.
pub async fn fetch_email(access_token: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("build Graph profile HTTP client")?;
    let resp = client
        .get(GRAPH_ME_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .context("GET Graph /me")?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("Graph /me returned HTTP {status}"));
    }
    let body = resp.text().await.context("read Graph /me response body")?;
    let me: GraphMe = serde_json::from_str(&body).context("parse Graph /me JSON")?;
    Ok(me
        .mail
        .filter(|s| !s.trim().is_empty())
        .or(me.user_principal_name)
        .unwrap_or_default())
}

// --- CalendarSource (background-refreshed snapshot) ----------------------

/// A [`CalendarSource`] backed by Microsoft Graph.
///
/// On construction a background task refreshes the cached snapshot every
/// [`REFRESH_INTERVAL_SECS`]; `upcoming()` just clones the latest good snapshot,
/// so it stays sync + non-blocking (no `block_on`). On a refresh error the last
/// good snapshot is retained and a warning is logged.
pub struct MicrosoftCalendarSource {
    snapshot: Arc<Mutex<Vec<UpcomingEvent>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl MicrosoftCalendarSource {
    /// Spawn the background refresh task on `handle` and return the source.
    ///
    /// `store` supplies (and persists refreshed) tokens; the task calls
    /// [`valid_access_token`] then [`fetch_events`] each tick until shutdown.
    pub fn spawn(
        store: Arc<dyn CalTokenStore>,
        token_operation: Arc<tokio::sync::Mutex<()>>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        let snapshot: Arc<Mutex<Vec<UpcomingEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let task_snapshot = Arc::downgrade(&snapshot);
        let cfg = Provider::Microsoft.config();

        let task = handle.spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(REFRESH_INTERVAL_SECS));
            let mut delta_link: Option<String> = None;
            let mut baseline_started_at = 0u64;
            let mut event_cache: HashMap<String, UpcomingEvent> = HashMap::new();

            loop {
                ticker.tick().await;
                let Some(task_snapshot) = task_snapshot.upgrade() else {
                    break;
                };
                let now = now_epoch_secs();
                match valid_access_token_serialized(
                    store.as_ref(),
                    &cfg,
                    now,
                    token_operation.as_ref(),
                )
                .await
                {
                    Ok(token) => {
                        let baseline_due = delta_link.is_none()
                            || now
                                >= baseline_started_at.saturating_add(BASELINE_REFRESH_SECS);
                        let request_link = if baseline_due {
                            None
                        } else {
                            delta_link.as_deref()
                        };
                        match fetch_events_with_delta(
                            &token,
                            now,
                            DEFAULT_LOOKAHEAD_SECS,
                            request_link,
                        )
                        .await
                        {
                            Ok(FetchMicrosoftEventsOutcome::Success {
                                events,
                                removed_ids,
                                delta_link: new_link,
                            }) => {
                                apply_event_changes(
                                    &mut event_cache,
                                    events,
                                    removed_ids,
                                    baseline_due,
                                );
                                if baseline_due {
                                    baseline_started_at = now;
                                }
                                delta_link = new_link;
                                *task_snapshot.lock().await =
                                    snapshot_from_cache(&event_cache, now);
                            }
                            Ok(FetchMicrosoftEventsOutcome::TokenGone) => {
                                tracing::info!("Microsoft Graph deltaLink expired (410 GONE) — performing baseline resync");
                                delta_link = None;
                                if let Ok(FetchMicrosoftEventsOutcome::Success {
                                    events,
                                    removed_ids,
                                    delta_link: new_link,
                                }) = fetch_events_with_delta(
                                    &token,
                                    now,
                                    DEFAULT_LOOKAHEAD_SECS,
                                    None,
                                )
                                .await
                                {
                                    apply_event_changes(
                                        &mut event_cache,
                                        events,
                                        removed_ids,
                                        true,
                                    );
                                    baseline_started_at = now;
                                    delta_link = new_link;
                                    *task_snapshot.lock().await =
                                        snapshot_from_cache(&event_cache, now);
                                }
                            }
                            Err(e) => tracing::warn!(
                                error = %e,
                                "microsoft calendar events fetch failed; keeping last snapshot"
                            ),
                        }
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "microsoft calendar token unavailable; keeping last snapshot"
                    ),
                }
            }
        });

        Self {
            snapshot,
            task: Some(task),
        }
    }

    /// Cancel and join the poller before credential removal. This prevents an
    /// in-flight access-token refresh from recreating a cleared keychain entry.
    pub async fn shutdown(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }

    /// Authorize Microsoft interactively and enrich the account email without
    /// persisting. The daemon stops the old poller before it installs these
    /// credentials, preventing stale refresh-token writes during reconnect.
    pub async fn authorize(open_browser: impl FnOnce(&str) -> Result<()>) -> Result<CalTokens> {
        let mut tokens = connect_interactive(Provider::Microsoft, open_browser).await?;
        // Best-effort email enrichment (the flow returns an empty email).
        if let Ok(email) = fetch_email(&tokens.access).await {
            tokens.email = email;
        }
        Ok(tokens)
    }

    /// Backward-compatible standalone connect. Daemon reconnects use
    /// [`Self::authorize`] so lifecycle ordering stays under one owner.
    pub async fn connect(open_browser: impl FnOnce(&str) -> Result<()>) -> Result<CalTokens> {
        let tokens = Self::authorize(open_browser).await?;
        let store = KeyringCalStore::new(Provider::Microsoft.keyring_service());
        store.save(&tokens)?;
        Ok(tokens)
    }
}

impl Drop for MicrosoftCalendarSource {
    fn drop(&mut self) {
        if let Some(task) = self.task.as_ref() {
            task.abort();
        }
    }
}

impl CalendarSource for MicrosoftCalendarSource {
    fn upcoming(&self, now_epoch_secs: u64) -> Vec<UpcomingEvent> {
        // Clone the cached snapshot without blocking: try_lock avoids any stall
        // if the background task momentarily holds the lock (it only holds it to
        // swap in a fresh Vec). On contention we return an empty list this tick
        // (fail-soft) rather than block the daemon poll.
        match self.snapshot.try_lock() {
            Ok(guard) => guard
                .iter()
                .filter(|event| {
                    event.start_epoch_secs >= now_epoch_secs
                        && event.start_epoch_secs
                            <= now_epoch_secs.saturating_add(DEFAULT_LOOKAHEAD_SECS)
                })
                .cloned()
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// Wall-clock now in epoch seconds (used by the background task).
fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_to_iso8601_utc_matches_known_instant() {
        // 2026-07-17T15:04:05Z = 1_784_300_645 epoch seconds.
        assert_eq!(epoch_to_iso8601_utc(1_784_300_645), "2026-07-17T15:04:05Z");
        // Unix epoch itself.
        assert_eq!(epoch_to_iso8601_utc(0), "1970-01-01T00:00:00Z");
        // A leap-year day: 2024-02-29T00:00:00Z = 1_709_164_800.
        assert_eq!(epoch_to_iso8601_utc(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn iso8601_to_epoch_roundtrips_and_handles_variants() {
        // Round-trip against the formatter.
        assert_eq!(
            iso8601_to_epoch("2026-07-17T15:04:05Z"),
            Some(1_784_300_645)
        );
        // Graph's fractional-seconds form, no Z (UTC via the Prefer header).
        assert_eq!(
            iso8601_to_epoch("2026-07-17T15:04:05.0000000"),
            Some(1_784_300_645)
        );
        // Explicit +00:00 offset.
        assert_eq!(
            iso8601_to_epoch("2026-07-17T15:04:05+00:00"),
            Some(1_784_300_645)
        );
        // A +02:00 offset should subtract two hours to reach UTC.
        assert_eq!(
            iso8601_to_epoch("2026-07-17T17:04:05+02:00"),
            Some(1_784_300_645)
        );
        // Unparseable → None (fail-soft).
        assert_eq!(iso8601_to_epoch("not-a-date"), None);
        assert_eq!(iso8601_to_epoch(""), None);
    }

    #[test]
    fn parse_calendar_view_maps_graph_sample() {
        // A hard-coded Graph v1.0 calendarView sample (UTC via Prefer header).
        let json = r#"{
          "value": [
            {
              "id": "AAMkEVT1",
              "subject": "Weekly Sync",
              "start": { "dateTime": "2026-07-17T15:00:00.0000000", "timeZone": "UTC" },
              "onlineMeeting": {
                "conferenceId": "987654321",
                "joinUrl": "https://teams.microsoft.com/l/meetup-join/opaque-token"
              },
              "organizer": {
                "emailAddress": { "name": "Alice Organizer", "address": "alice@contoso.com" }
              },
              "attendees": [
                {
                  "type": "required",
                  "emailAddress": { "name": "Bob Attendee", "address": "bob@contoso.com" },
                  "status": { "response": "accepted" }
                },
                {
                  "type": "required",
                  "emailAddress": { "name": "Alice Organizer", "address": "alice@contoso.com" }
                }
              ]
            }
          ]
        }"#;

        let events = parse_calendar_view(json).expect("parse ok");
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.id, "AAMkEVT1");
        assert_eq!(ev.provider, CalendarProvider::Microsoft);
        assert_eq!(ev.provider_event_id, "AAMkEVT1");
        assert_eq!(ev.meeting_id, "987654321");
        assert_eq!(
            ev.join_url,
            "https://teams.microsoft.com/l/meetup-join/opaque-token"
        );
        assert_eq!(ev.title, "Weekly Sync");
        // 2026-07-17T15:00:00Z = 1_784_300_400.
        assert_eq!(ev.start_epoch_secs, 1_784_300_400);
        assert_eq!(ev.participants.len(), 2);
        let mut attendee_emails = ev
            .participants
            .iter()
            .map(|participant| participant.email.as_str())
            .collect::<Vec<_>>();
        attendee_emails.sort_unstable();
        assert_eq!(attendee_emails, ["alice@contoso.com", "bob@contoso.com"]);

        let bob = ev
            .participants
            .iter()
            .find(|p| p.email == "bob@contoso.com")
            .expect("bob present");
        assert_eq!(bob.name, "Bob Attendee");
        assert!(!bob.is_organizer);
        assert_eq!(bob.response, cue_core::calendar::ResponseStatus::Accepted);

        let alice = ev
            .participants
            .iter()
            .find(|p| p.email == "alice@contoso.com")
            .expect("alice present");
        assert_eq!(alice.name, "Alice Organizer");
        assert!(alice.is_organizer, "organizer flagged among attendees");
    }

    #[test]
    fn parse_calendar_view_appends_absent_organizer_and_skips_unparseable() {
        // First event: organizer NOT in attendees → appended. Second event:
        // missing start.dateTime → skipped (fail-soft per item).
        let json = r#"{
          "value": [
            {
              "id": "E1",
              "subject": "1:1",
              "start": { "dateTime": "2026-07-17T16:30:00.0000000" },
              "organizer": {
                "emailAddress": { "name": "Carol Chair", "address": "carol@contoso.com" }
              },
              "attendees": [
                { "emailAddress": { "name": "Dan", "address": "dan@contoso.com" } }
              ]
            },
            {
              "id": "E2",
              "subject": "Broken",
              "attendees": []
            }
          ]
        }"#;

        let events = parse_calendar_view(json).expect("parse ok");
        assert_eq!(events.len(), 1, "unparseable-start event skipped");
        let ev = &events[0];
        assert_eq!(ev.id, "E1");
        assert_eq!(ev.participants.len(), 2, "organizer appended to roster");
        let carol = ev
            .participants
            .iter()
            .find(|p| p.email == "carol@contoso.com")
            .expect("organizer appended");
        assert!(carol.is_organizer);
    }

    #[test]
    fn teams_join_url_is_not_parsed_as_a_meeting_id() {
        let events = parse_calendar_view(
            r#"{"value":[{
                "id":"immutable-event-id",
                "start":{"dateTime":"2026-07-17T16:30:00Z"},
                "onlineMeeting":{
                    "joinUrl":"https://teams.microsoft.com/l/meetup-join/opaque-token"
                }
            }]}"#,
        )
        .unwrap();
        assert_eq!(events[0].id, "immutable-event-id");
        assert_eq!(
            events[0].join_url,
            "https://teams.microsoft.com/l/meetup-join/opaque-token"
        );
        assert!(
            events[0].meeting_id.is_empty(),
            "meeting id requires Graph's structured conferenceId"
        );
    }

    #[test]
    fn parse_calendar_view_empty_value_is_ok() {
        let events = parse_calendar_view(r#"{"value":[]}"#).expect("parse ok");
        assert!(events.is_empty());
        // Missing `value` also fails soft to empty.
        let events = parse_calendar_view(r#"{}"#).expect("parse ok");
        assert!(events.is_empty());
    }

    #[test]
    fn incremental_changes_preserve_unchanged_events_and_apply_removals() {
        let baseline = serde_json::from_str::<CalendarViewResponse>(
            r#"{"value":[
                {"id":"keep","subject":"Keep",
                 "start":{"dateTime":"2026-07-17T16:30:00Z"}},
                {"id":"change","subject":"Old",
                 "start":{"dateTime":"2026-07-17T16:31:00Z"}}
            ]}"#,
        )
        .unwrap();
        let (events, removed) = event_changes(baseline.value);
        let mut cache = HashMap::new();
        apply_event_changes(&mut cache, events, removed, true);

        let delta = serde_json::from_str::<CalendarViewResponse>(
            r#"{"value":[
                {"id":"change","subject":"New",
                 "start":{"dateTime":"2026-07-17T16:32:00Z"}},
                {"id":"keep","@removed":{"reason":"deleted"}}
            ]}"#,
        )
        .unwrap();
        let (events, removed) = event_changes(delta.value);
        apply_event_changes(&mut cache, events, removed, false);

        assert_eq!(cache.len(), 1);
        assert_eq!(cache["change"].title, "New");
        assert!(!cache.contains_key("keep"));
    }

    #[test]
    fn graph_delta_url_must_stay_on_official_https_origin() {
        assert!(validate_graph_delta_url(
            "https://graph.microsoft.com/v1.0/me/calendarView/delta?$deltatoken=abc"
        )
        .is_ok());
        assert!(validate_graph_delta_url(
            "https://attacker.example/v1.0/me/calendarView/delta?$deltatoken=abc"
        )
        .is_err());
        assert!(validate_graph_delta_url(
            "http://graph.microsoft.com/v1.0/me/calendarView/delta?$deltatoken=abc"
        )
        .is_err());
        assert!(validate_graph_delta_url(
            "https://graph.microsoft.com/v1.0/me/messages?$deltatoken=abc"
        )
        .is_err());
    }

    #[test]
    fn calendar_requests_require_immutable_graph_event_ids() {
        let request =
            with_graph_calendar_preferences(reqwest::Client::new().get(GRAPH_CALENDAR_DELTA_URL))
                .build()
                .unwrap();
        let prefer = request
            .headers()
            .get("Prefer")
            .and_then(|value| value.to_str().ok())
            .expect("Prefer header");
        assert!(prefer.contains("outlook.timezone=\"UTC\""));
        assert!(prefer.contains("IdType=\"ImmutableId\""));
        assert!(prefer.contains("odata.maxpagesize=100"));
    }
}
