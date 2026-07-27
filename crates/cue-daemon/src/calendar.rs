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
pub use cue_core::calendar::{CalendarProvider, CalendarSource, Participant, UpcomingEvent};

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

/// Pick the calendar source the daemon should poll:
/// 1. the env fake when `BLUEY_CALENDAR_FAKE_EVENTS` is set (the deterministic
///    test hook wins first so a test never races a real calendar);
/// 2. a dynamic cloud source (feature `cloud-calendar`) that can activate or
///    deactivate Google/Microsoft immediately after onboarding;
/// 3. a no-op (the trigger stays dormant rather than erroring).
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
        if let Some(source) = DynamicCloudSource::new() {
            return Box::new(source);
        }
    }
    Box::new(NoopSource)
}

#[cfg(feature = "cloud-calendar")]
struct CloudSources {
    google_initialized: bool,
    microsoft_initialized: bool,
    google: Option<cue_calendar_cloud::google::GoogleCalendarSource>,
    microsoft: Option<cue_calendar_cloud::microsoft::MicrosoftCalendarSource>,
    google_store: Option<std::sync::Arc<dyn cue_calendar_cloud::CalTokenStore>>,
    microsoft_store: Option<std::sync::Arc<dyn cue_calendar_cloud::CalTokenStore>>,
    google_token_operation: std::sync::Arc<tokio::sync::Mutex<()>>,
    microsoft_token_operation: std::sync::Arc<tokio::sync::Mutex<()>>,
}

#[cfg(feature = "cloud-calendar")]
impl Default for CloudSources {
    fn default() -> Self {
        Self {
            google_initialized: false,
            microsoft_initialized: false,
            google: None,
            microsoft: None,
            google_store: None,
            microsoft_store: None,
            google_token_operation: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            microsoft_token_operation: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

#[cfg(feature = "cloud-calendar")]
fn cloud_sources() -> std::sync::Arc<std::sync::Mutex<CloudSources>> {
    static SOURCES: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<CloudSources>>> =
        std::sync::OnceLock::new();
    std::sync::Arc::clone(
        SOURCES.get_or_init(|| std::sync::Arc::new(std::sync::Mutex::new(CloudSources::default()))),
    )
}

#[cfg(feature = "cloud-calendar")]
fn cached_store_from_keychain(
    provider: cue_calendar_cloud::Provider,
) -> anyhow::Result<std::sync::Arc<dyn cue_calendar_cloud::CalTokenStore>> {
    use cue_calendar_cloud::{CachedCalStore, KeyringCalStore};
    use std::sync::Arc;

    let backend: Arc<dyn cue_calendar_cloud::CalTokenStore> =
        Arc::new(KeyringCalStore::new(provider.keyring_service()));
    let store = CachedCalStore::load_from(backend)?;
    Ok(Arc::new(store))
}

#[cfg(feature = "cloud-calendar")]
fn new_cached_store_with_tokens(
    provider: cue_calendar_cloud::Provider,
    tokens: cue_calendar_cloud::CalTokens,
) -> std::sync::Arc<dyn cue_calendar_cloud::CalTokenStore> {
    use cue_calendar_cloud::{CachedCalStore, KeyringCalStore};
    use std::sync::Arc;

    let backend: Arc<dyn cue_calendar_cloud::CalTokenStore> =
        Arc::new(KeyringCalStore::new(provider.keyring_service()));
    Arc::new(CachedCalStore::with_tokens(backend, tokens))
}

/// Complete the destructive half of a reconnect in one auditable order:
/// terminate the previous poller, then serialize the replacement token write.
/// The caller may only publish/spawn the replacement source after this returns.
#[cfg(feature = "cloud-calendar")]
async fn stop_then_save_calendar_tokens<S, Shutdown, ShutdownFuture>(
    previous: Option<S>,
    store: &std::sync::Arc<dyn cue_calendar_cloud::CalTokenStore>,
    token_operation: &std::sync::Arc<tokio::sync::Mutex<()>>,
    tokens: &cue_calendar_cloud::CalTokens,
    shutdown: Shutdown,
) -> anyhow::Result<()>
where
    Shutdown: FnOnce(S) -> ShutdownFuture,
    ShutdownFuture: std::future::Future<Output = ()>,
{
    if let Some(previous) = previous {
        shutdown(previous).await;
    }
    let _token_guard = token_operation.lock().await;
    store.save(tokens)
}

#[cfg(feature = "cloud-calendar")]
fn initialize_cloud_sources(
    sources: &std::sync::Arc<std::sync::Mutex<CloudSources>>,
    handle: &tokio::runtime::Handle,
) {
    use cue_calendar_cloud::google::GoogleCalendarSource;
    use cue_calendar_cloud::microsoft::MicrosoftCalendarSource;
    use cue_calendar_cloud::Provider;

    let (load_google, load_microsoft) = match sources.lock() {
        Ok(guard) => (!guard.google_initialized, !guard.microsoft_initialized),
        Err(error) => {
            tracing::warn!(%error, "calendar source registry mutex poisoned");
            return;
        }
    };

    if load_google {
        match Provider::Google
            .config()
            .validate()
            .and_then(|_| cached_store_from_keychain(Provider::Google))
        {
            Ok(store) => {
                if let Ok(mut guard) = sources.lock() {
                    if !guard.google_initialized {
                        let connected = store.load().ok().flatten().is_some();
                        let token_operation = std::sync::Arc::clone(&guard.google_token_operation);
                        let source = connected.then(|| {
                            GoogleCalendarSource::spawn(
                                std::sync::Arc::clone(&store),
                                token_operation,
                                handle.clone(),
                            )
                        });
                        guard.google = source;
                        guard.google_store = Some(store);
                        guard.google_initialized = true;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Google calendar source initialization deferred");
            }
        }
    }
    if load_microsoft {
        match Provider::Microsoft
            .config()
            .validate()
            .and_then(|_| cached_store_from_keychain(Provider::Microsoft))
        {
            Ok(store) => {
                if let Ok(mut guard) = sources.lock() {
                    if !guard.microsoft_initialized {
                        let connected = store.load().ok().flatten().is_some();
                        let token_operation =
                            std::sync::Arc::clone(&guard.microsoft_token_operation);
                        let source = connected.then(|| {
                            MicrosoftCalendarSource::spawn(
                                std::sync::Arc::clone(&store),
                                token_operation,
                                handle.clone(),
                            )
                        });
                        guard.microsoft = source;
                        guard.microsoft_store = Some(store);
                        guard.microsoft_initialized = true;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Microsoft calendar source initialization deferred");
            }
        }
    }
}

/// Activate a newly connected provider immediately. Without this hook the
/// calendar poll retained the `NoopSource` selected at daemon startup, so normal
/// onboarding did nothing until Bluey restarted.
#[cfg(feature = "cloud-calendar")]
pub async fn activate_cloud_provider(
    provider: &str,
    tokens: cue_calendar_cloud::CalTokens,
) -> anyhow::Result<()> {
    use cue_calendar_cloud::google::GoogleCalendarSource;
    use cue_calendar_cloud::microsoft::MicrosoftCalendarSource;
    use cue_calendar_cloud::Provider;

    let handle = tokio::runtime::Handle::try_current()
        .map_err(|error| anyhow::anyhow!("calendar runtime unavailable: {error}"))?;
    let sources = cloud_sources();
    initialize_cloud_sources(&sources, &handle);
    match provider {
        "google" => {
            let (previous, existing_store, token_operation) = {
                let mut guard = sources
                    .lock()
                    .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
                (
                    guard.google.take(),
                    guard.google_store.clone(),
                    std::sync::Arc::clone(&guard.google_token_operation),
                )
            };
            let store = existing_store
                .unwrap_or_else(|| new_cached_store_with_tokens(Provider::Google, tokens.clone()));
            let save_result = stop_then_save_calendar_tokens(
                previous,
                &store,
                &token_operation,
                &tokens,
                GoogleCalendarSource::shutdown,
            )
            .await;
            if let Err(error) = save_result {
                if let Ok(mut guard) = sources.lock() {
                    guard.google_initialized = false;
                }
                return Err(error.context("persist replacement Google calendar credentials"));
            }
            let source =
                GoogleCalendarSource::spawn(std::sync::Arc::clone(&store), token_operation, handle);
            let mut guard = sources
                .lock()
                .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
            guard.google = Some(source);
            guard.google_store = Some(store);
            guard.google_initialized = true;
        }
        "microsoft" => {
            let (previous, existing_store, token_operation) = {
                let mut guard = sources
                    .lock()
                    .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
                (
                    guard.microsoft.take(),
                    guard.microsoft_store.clone(),
                    std::sync::Arc::clone(&guard.microsoft_token_operation),
                )
            };
            let store = existing_store.unwrap_or_else(|| {
                new_cached_store_with_tokens(Provider::Microsoft, tokens.clone())
            });
            let save_result = stop_then_save_calendar_tokens(
                previous,
                &store,
                &token_operation,
                &tokens,
                MicrosoftCalendarSource::shutdown,
            )
            .await;
            if let Err(error) = save_result {
                if let Ok(mut guard) = sources.lock() {
                    guard.microsoft_initialized = false;
                }
                return Err(error.context("persist replacement Microsoft calendar credentials"));
            }
            let source = MicrosoftCalendarSource::spawn(
                std::sync::Arc::clone(&store),
                token_operation,
                handle,
            );
            let mut guard = sources
                .lock()
                .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
            guard.microsoft = Some(source);
            guard.microsoft_store = Some(store);
            guard.microsoft_initialized = true;
        }
        other => anyhow::bail!("unknown calendar provider \"{other}\""),
    }
    Ok(())
}

/// Stop and join a provider, then clear the same shared store its poller used.
/// Merely dropping the snapshot or clearing a separate keyring handle is
/// insufficient: an in-flight refresh could otherwise recreate credentials.
#[cfg(feature = "cloud-calendar")]
pub async fn deactivate_cloud_provider(provider: &str) -> anyhow::Result<()> {
    use cue_calendar_cloud::Provider;

    let sources = cloud_sources();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        initialize_cloud_sources(&sources, &handle);
    }
    match provider {
        "google" => {
            let (previous, existing_store, token_operation) = {
                let mut guard = sources
                    .lock()
                    .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
                guard.google_initialized = true;
                (
                    guard.google.take(),
                    guard.google_store.clone(),
                    std::sync::Arc::clone(&guard.google_token_operation),
                )
            };
            if let Some(previous) = previous {
                previous.shutdown().await;
            }
            let store = match existing_store {
                Some(store) => store,
                None => cached_store_from_keychain(Provider::Google)?,
            };
            {
                let _token_guard = token_operation.lock().await;
                store.clear()?;
            }
            if let Ok(mut guard) = sources.lock() {
                guard.google_store = Some(store);
            }
        }
        "microsoft" => {
            let (previous, existing_store, token_operation) = {
                let mut guard = sources
                    .lock()
                    .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
                guard.microsoft_initialized = true;
                (
                    guard.microsoft.take(),
                    guard.microsoft_store.clone(),
                    std::sync::Arc::clone(&guard.microsoft_token_operation),
                )
            };
            if let Some(previous) = previous {
                previous.shutdown().await;
            }
            let store = match existing_store {
                Some(store) => store,
                None => cached_store_from_keychain(Provider::Microsoft)?,
            };
            {
                let _token_guard = token_operation.lock().await;
                store.clear()?;
            }
            if let Ok(mut guard) = sources.lock() {
                guard.microsoft_store = Some(store);
            }
        }
        other => anyhow::bail!("unknown calendar provider \"{other}\""),
    }
    Ok(())
}

/// Validate one provider through the exact store + refresh serialization lock
/// used by its live poller. Returns the connected account email, `None` when no
/// credentials are stored, and an error when stored credentials cannot refresh.
#[cfg(feature = "cloud-calendar")]
pub async fn validate_cloud_provider(
    provider: &str,
    now_epoch: u64,
) -> anyhow::Result<Option<String>> {
    use cue_calendar_cloud::{valid_access_token_serialized, Provider};

    let provider_enum = match provider {
        "google" => Provider::Google,
        "microsoft" => Provider::Microsoft,
        other => anyhow::bail!("unknown calendar provider \"{other}\""),
    };
    let config = provider_enum.config();
    config.validate()?;

    let handle = tokio::runtime::Handle::try_current()
        .map_err(|error| anyhow::anyhow!("calendar runtime unavailable: {error}"))?;
    let sources = cloud_sources();
    initialize_cloud_sources(&sources, &handle);
    let (store, token_operation) = {
        let guard = sources
            .lock()
            .map_err(|_| anyhow::anyhow!("calendar source registry mutex poisoned"))?;
        match provider_enum {
            Provider::Google => (
                guard.google_store.clone(),
                std::sync::Arc::clone(&guard.google_token_operation),
            ),
            Provider::Microsoft => (
                guard.microsoft_store.clone(),
                std::sync::Arc::clone(&guard.microsoft_token_operation),
            ),
        }
    };
    let Some(store) = store else {
        return Ok(None);
    };
    if store.load()?.is_none() {
        return Ok(None);
    }

    valid_access_token_serialized(store.as_ref(), &config, now_epoch, token_operation.as_ref())
        .await?;
    Ok(store.load()?.map(|tokens| tokens.email))
}

#[cfg(feature = "cloud-calendar")]
struct DynamicCloudSource {
    sources: std::sync::Arc<std::sync::Mutex<CloudSources>>,
    handle: tokio::runtime::Handle,
}

#[cfg(feature = "cloud-calendar")]
impl DynamicCloudSource {
    fn new() -> Option<Self> {
        let handle = tokio::runtime::Handle::try_current().ok()?;
        let sources = cloud_sources();
        initialize_cloud_sources(&sources, &handle);
        Some(Self { sources, handle })
    }
}

#[cfg(feature = "cloud-calendar")]
impl CalendarSource for DynamicCloudSource {
    fn upcoming(&self, now_epoch_secs: u64) -> Vec<UpcomingEvent> {
        // Retry only providers whose prior initialization ended in a transient
        // configuration/keychain failure.
        initialize_cloud_sources(&self.sources, &self.handle);
        let Ok(guard) = self.sources.lock() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        if let Some(source) = guard.google.as_ref() {
            events.extend(
                source
                    .upcoming(now_epoch_secs)
                    .into_iter()
                    .map(|event| namespace_provider_event(event, CalendarProvider::Google)),
            );
        }
        if let Some(source) = guard.microsoft.as_ref() {
            events.extend(
                source
                    .upcoming(now_epoch_secs)
                    .into_iter()
                    .map(|event| namespace_provider_event(event, CalendarProvider::Microsoft)),
            );
        }
        events.sort_by_key(|event| event.start_epoch_secs);
        events
    }
}

/// Give an occurrence a globally unique Bluey identity without corrupting the
/// exact provider id that calendar/MCP connectors need for lookup.
#[cfg(feature = "cloud-calendar")]
fn namespace_provider_event(mut event: UpcomingEvent, provider: CalendarProvider) -> UpcomingEvent {
    let provider_event_id = if event.provider_event_id.trim().is_empty() {
        event.id.clone()
    } else {
        event.provider_event_id.clone()
    };
    event.provider = provider;
    event.provider_event_id = provider_event_id.clone();
    event.id = format!("{}:{provider_event_id}", provider.as_str());
    event
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
                let (title, start) = entry.trim().rsplit_once('@')?;
                let start: u64 = start.trim().parse().ok()?;
                let provider_event_id = format!("fake-{}", title.trim());
                Some(UpcomingEvent {
                    id: format!("fake:{provider_event_id}"),
                    provider: CalendarProvider::Fake,
                    provider_event_id,
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
/// even if the next meeting is far off. Cloud sources refresh their snapshots
/// independently, so the scheduler must revisit those snapshots at the normal
/// poll cadence. Otherwise a source activated after onboarding, or a newly
/// created meeting, could remain invisible here for five minutes.
pub const SAFETY_POLL_SECS: u64 = POLL_SECS;

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

    #[cfg(feature = "cloud-calendar")]
    #[test]
    fn dynamic_source_namespaces_ids_without_corrupting_provider_identity() {
        for (provider, expected_prefix) in [
            (CalendarProvider::Google, "google"),
            (CalendarProvider::Microsoft, "microsoft"),
        ] {
            let raw = format!("{expected_prefix}-provider-event");
            let event = UpcomingEvent {
                id: raw.clone(),
                provider,
                provider_event_id: raw.clone(),
                ..Default::default()
            };

            let namespaced = namespace_provider_event(event, provider);
            assert_eq!(namespaced.id, format!("{expected_prefix}:{raw}"));
            assert_eq!(namespaced.provider, provider);
            assert_eq!(namespaced.provider_event_id, raw);
        }
    }

    #[cfg(feature = "cloud-calendar")]
    #[test]
    fn dynamic_source_recovers_raw_id_for_legacy_provider_event() {
        let event = UpcomingEvent {
            id: "legacy-graph-id".to_string(),
            ..Default::default()
        };
        let namespaced = namespace_provider_event(event, CalendarProvider::Microsoft);
        assert_eq!(namespaced.id, "microsoft:legacy-graph-id");
        assert_eq!(namespaced.provider_event_id, "legacy-graph-id");
    }

    #[cfg(feature = "cloud-calendar")]
    #[tokio::test]
    async fn reconnect_stops_old_source_before_publishing_new_tokens() {
        use cue_calendar_cloud::{CalTokenStore, CalTokens};
        use std::sync::{Arc, Mutex};

        struct RecordingStore {
            lifecycle: Arc<Mutex<Vec<&'static str>>>,
        }

        impl CalTokenStore for RecordingStore {
            fn save(&self, _tokens: &CalTokens) -> anyhow::Result<()> {
                self.lifecycle.lock().unwrap().push("save");
                Ok(())
            }

            fn load(&self) -> anyhow::Result<Option<CalTokens>> {
                Ok(None)
            }

            fn clear(&self) -> anyhow::Result<()> {
                Ok(())
            }
        }

        let lifecycle = Arc::new(Mutex::new(Vec::new()));
        let store: Arc<dyn CalTokenStore> = Arc::new(RecordingStore {
            lifecycle: Arc::clone(&lifecycle),
        });
        let token_operation = Arc::new(tokio::sync::Mutex::new(()));
        let tokens = CalTokens {
            access: "replacement-access".into(),
            refresh: "replacement-refresh".into(),
            expires_at_epoch: 1_800_000_000,
            email: "person@example.com".into(),
        };
        let shutdown_lifecycle = Arc::clone(&lifecycle);

        stop_then_save_calendar_tokens(
            Some(()),
            &store,
            &token_operation,
            &tokens,
            move |_| async move {
                shutdown_lifecycle.lock().unwrap().push("stop");
            },
        )
        .await
        .unwrap();

        assert_eq!(*lifecycle.lock().unwrap(), vec!["stop", "save"]);
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

        // Event whose warm moment is 120s away is capped at the normal poll
        // cadence so a newly-refreshed cloud snapshot is observed promptly.
        let soon = vec![event("y", now + WARM_LEAD_SECS + 120)];
        assert_eq!(next_wake_secs(&soon, &fired, now), POLL_SECS);

        // Already inside the lead window → wake now (0).
        let due = vec![event("z", now + WARM_LEAD_SECS - 10)];
        assert_eq!(next_wake_secs(&due, &fired, now), 0);

        // The soonest of several drives the sleep.
        let many = vec![
            event("a", now + WARM_LEAD_SECS + 25),
            event("b", now + WARM_LEAD_SECS + 5),
            event("c", now + WARM_LEAD_SECS + 15),
        ];
        assert_eq!(next_wake_secs(&many, &fired, now), 5);

        // A fired event is ignored → the NEXT unfired one drives the sleep.
        let mut fired2 = HashSet::new();
        fired2.insert(fired_key(&many[1])); // b (5s) consumed
        assert_eq!(next_wake_secs(&many, &fired2, now), 15); // now c
    }

    #[test]
    fn newly_synced_events_are_observed_within_the_normal_poll_cadence() {
        let now = 3_000_000;
        let fired = HashSet::new();

        assert_eq!(next_wake_secs(&[], &fired, now), POLL_SECS);
        assert_eq!(
            next_wake_secs(&[event("far-away", now + 10_000)], &fired, now),
            POLL_SECS
        );
    }
}
