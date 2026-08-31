use cue_core::ipc::{
    DaemonMutationFence, DaemonRequest, DaemonResponse, DaemonSessionLifecycle, DaemonSessionRecord,
};
use cue_core::session::{Session, SessionStatus};
use cue_core::{AudioCaptureState, AudioPipelineStatus, AudioSourceKind, AudioSourceState};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::watch;
use uuid::Uuid;

use crate::DbState;

/// Shared state for the currently active session.
///
/// Separate from `DbState` so callers can hold an active-session lock
/// independently of the DB connection lock. The active session is a soft
/// selection in the dashboard UI; the DB is the source of truth for session
/// data itself.
pub struct ActiveSessionState(pub(crate) Mutex<Option<ActiveSessionSelection>>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DashboardOwner {
    Local,
    SignedIn(String),
}

impl DashboardOwner {
    fn db_owner_id(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::SignedIn(id) => Some(id.as_str()),
        }
    }

    fn event_key(&self) -> String {
        match self {
            Self::Local => "local".to_string(),
            Self::SignedIn(id) => format!("account:{id}"),
        }
    }

    fn signed_in(&self) -> bool {
        matches!(self, Self::SignedIn(_))
    }

    pub(crate) fn owns_meeting(&self, meeting_owner: Option<&str>) -> bool {
        let meeting_owner = meeting_owner
            .map(str::trim)
            .filter(|owner| !owner.is_empty());
        self.db_owner_id() == meeting_owner
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveSessionSelection {
    owner: DashboardOwner,
    id: Option<Uuid>,
}

pub(crate) struct DashboardOwnerCache {
    pub(crate) owner: Option<DashboardOwner>,
    transitioning: bool,
    transition_generation: u64,
}

pub struct DashboardOwnerState(pub(crate) Mutex<DashboardOwnerCache>);

#[derive(Clone, Serialize)]
pub struct DashboardOwnerPayload {
    pub owner_key: String,
    pub signed_in: bool,
    pub available: bool,
}

impl DashboardOwnerPayload {
    fn available(owner: &DashboardOwner) -> Self {
        Self {
            owner_key: owner.event_key(),
            signed_in: owner.signed_in(),
            available: true,
        }
    }

    fn unavailable() -> Self {
        Self {
            owner_key: "unavailable".to_string(),
            signed_in: false,
            available: false,
        }
    }
}

/// Payload emitted on `session:switched` whenever the active session changes.
///
/// Separate struct (not `Session`) because the consumer typically already has
/// full `Session` data; the switch event just signals "the selection moved".
#[derive(Clone, Serialize)]
pub struct SessionSwitchedPayload {
    /// Session id now active, or `None` if active selection was cleared.
    pub id: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct BalanceSnapshotPayload {
    pub balance_cents: i64,
    pub balance_label: String,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
    pub low_balance_warning: bool,
}

#[derive(Clone, Serialize)]
pub struct AccountMePayload {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontendErrorSource {
    TauriInvoke,
    WindowError,
    UnhandledRejection,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontendErrorCategory {
    InvokeRejected,
    RuntimeError,
    UnhandledRejection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendErrorPayload {
    pub source: FrontendErrorSource,
    pub category: FrontendErrorCategory,
    #[serde(default)]
    pub command: Option<String>,
}

// ===== Daemon IPC helper =====

/// Send a request to the running daemon over TCP and return the response.
pub(crate) async fn daemon_ipc(request: DaemonRequest) -> Result<DaemonResponse, String> {
    let trace_id = dashboard_trace_id();
    daemon_ipc_with_trace(request, &trace_id).await
}

async fn daemon_ipc_with_trace(
    request: DaemonRequest,
    trace_id: &str,
) -> Result<DaemonResponse, String> {
    let compatibility_addr = daemon_addr();
    daemon_ipc_with_trace_to_endpoint(request, trace_id, compatibility_addr.as_deref()).await
}

#[cfg(test)]
async fn daemon_ipc_with_trace_to_addr(
    request: DaemonRequest,
    trace_id: &str,
    addr: &str,
) -> Result<DaemonResponse, String> {
    daemon_ipc_with_trace_to_endpoint(request, trace_id, Some(addr)).await
}

async fn daemon_ipc_with_trace_to_endpoint(
    request: DaemonRequest,
    trace_id: &str,
    compatibility_addr: Option<&str>,
) -> Result<DaemonResponse, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|error| error.to_string())?;
    cue_core::ipc_transport::request_daemon(
        &paths,
        compatibility_addr,
        request.with_trace_id(trace_id.to_string()),
    )
    .await
    .map_err(|error| error.to_string())
}

fn daemon_addr() -> Option<String> {
    std::env::var("BLUEY_DAEMON_ADDR")
        .or_else(|_| std::env::var("CUE_DAEMON_ADDR"))
        .ok()
        .filter(|addr| !addr.trim().is_empty())
}

fn dashboard_trace_id() -> String {
    cue_core::new_trace_id()
}

struct DashboardAccountCloudContext {
    paths: cue_core::app_paths::AppPaths,
    owner_account_id: String,
    credential_generation: u64,
    client: cue_cloud_client::CloudClient,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DashboardAccountFileStamp {
    present: bool,
    provider: String,
    cloud_account_id: Option<String>,
    user_id: String,
    credential_generation: u64,
    token_configured: bool,
}

impl DashboardAccountFileStamp {
    fn from_account(account: Option<&cue_core::AccountConfig>) -> Self {
        let Some(account) = account else {
            return Self {
                present: false,
                provider: String::new(),
                cloud_account_id: None,
                user_id: String::new(),
                credential_generation: 0,
                token_configured: false,
            };
        };
        Self {
            present: true,
            provider: account.provider.trim().to_ascii_lowercase(),
            cloud_account_id: account
                .cloud_account_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            user_id: account.user_id.trim().to_string(),
            credential_generation: account.credential_generation,
            token_configured: account.token_configured(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DashboardOwnerIdentity {
    owner: DashboardOwner,
    credential_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DashboardOwnerSnapshot {
    identity: DashboardOwnerIdentity,
    account_file_stamp: DashboardAccountFileStamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DashboardOwnerGuard {
    identity: DashboardOwnerIdentity,
    account_file_stamp: DashboardAccountFileStamp,
    transition_generation: u64,
}

#[derive(Clone)]
struct DashboardOwnerStreamToken {
    cancelled: watch::Receiver<bool>,
}

struct DashboardOwnerStreamFence {
    token: DashboardOwnerStreamToken,
    stop: Arc<AtomicBool>,
    _monitor: tokio::task::JoinHandle<()>,
}

impl DashboardAccountCloudContext {
    fn snapshot(trace_id: &str) -> Result<Self, String> {
        let paths = cue_core::app_paths::AppPaths::discover()
            .map_err(|e| format!("account store unavailable: {e}"))?;
        let account = cue_core::load_account(&paths)
            .map_err(|e| format!("account profile unavailable: {e}"))?
            .ok_or_else(|| "Sign in before changing cloud data controls.".to_string())?;
        let owner_account_id = account
            .owner_account_id()
            .map(ToString::to_string)
            .ok_or_else(|| "Sign in before changing cloud data controls.".to_string())?;
        let access = account
            .access_token
            .clone()
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| "Sign in before changing cloud data controls.".to_string())?;
        let tokens = cue_cloud_client::Tokens {
            access,
            refresh: account.refresh_token.clone().unwrap_or_default(),
            email: account.user_id.clone(),
        };
        let store = cue_cloud_client::tokens::MemoryStore::new();
        cue_cloud_client::TokenStore::save(&store, &tokens)
            .map_err(|e| format!("account credentials unavailable: {e}"))?;
        let config = cue_cloud_client::client::ClientConfig {
            base_url: account.api_url.clone(),
            trace_id: Some(trace_id.to_string()),
            ..Default::default()
        };
        let client = cue_cloud_client::CloudClient::new(config, Arc::new(store))
            .map_err(|e| format!("account store unavailable: {e}"))?;
        Ok(Self {
            paths,
            owner_account_id,
            credential_generation: account.credential_generation,
            client,
        })
    }

    fn ensure_current(&self) -> Result<(), String> {
        let account = cue_core::load_account(&self.paths)
            .map_err(|e| format!("account profile unavailable: {e}"))?
            .ok_or_else(account_changed_message)?;
        let owner_account_id = account
            .owner_account_id()
            .ok_or_else(account_changed_message)?;
        if owner_account_id != self.owner_account_id.as_str()
            || account.credential_generation != self.credential_generation
        {
            return Err(account_changed_message());
        }
        Ok(())
    }
}

fn account_changed_message() -> String {
    "The signed-in account changed. No cloud data control was changed; try again.".to_string()
}

fn captured_managed_llm_client(
    app: &AppHandle,
    owner_guard: &DashboardOwnerGuard,
    trace_id: &str,
) -> Result<Option<cue_cloud_client::CloudClient>, String> {
    owner_guard.with_current(app, || {
        if !owner_guard.identity.owner.signed_in() {
            return Ok(None);
        }
        let account_context = DashboardAccountCloudContext::snapshot(trace_id)?;
        if owner_guard.identity.owner.db_owner_id()
            != Some(account_context.owner_account_id.as_str())
            || owner_guard.identity.credential_generation != account_context.credential_generation
        {
            return Err(account_changed_message());
        }
        Ok(Some(account_context.client))
    })
}

fn cloud_client_with_trace(trace_id: &str) -> Result<cue_cloud_client::CloudClient, String> {
    let paths = cue_core::app_paths::AppPaths::discover()
        .map_err(|e| format!("account store unavailable: {e}"))?;
    let account =
        cue_core::load_account(&paths).map_err(|e| format!("account profile unavailable: {e}"))?;
    let mut config = cue_cloud_client::client::ClientConfig {
        trace_id: Some(trace_id.to_string()),
        ..Default::default()
    };
    if let Some(account) = account.as_ref() {
        if !account.api_url.trim().is_empty() {
            config.base_url = account.api_url.clone();
        }
    }
    let client = cue_cloud_client::CloudClient::new(
        config.clone(),
        Arc::new(cue_cloud_client::SecureAccountStore::new(paths)),
    )
    .map_err(|e| format!("account store unavailable: {e}"))?;
    if client.current_tokens().is_some() || !legacy_keyring_fallback_enabled() {
        return Ok(client);
    }

    cue_cloud_client::CloudClient::new(
        config,
        Arc::new(cue_cloud_client::tokens::KeyringStore::new()),
    )
    .map_err(|e| format!("legacy account keyring unavailable: {e}"))
}

fn legacy_keyring_fallback_enabled() -> bool {
    std::env::var("BLUEY_LEGACY_KEYRING_FALLBACK")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

struct CapturedLegacyKeyringCredentials {
    store: cue_cloud_client::tokens::KeyringStore,
    snapshot: cue_cloud_client::CredentialSnapshot,
}

fn capture_legacy_keyring_credentials_if_enabled(
) -> Result<Option<CapturedLegacyKeyringCredentials>, String> {
    if !legacy_keyring_fallback_enabled() {
        return Ok(None);
    }
    let store = cue_cloud_client::tokens::KeyringStore::new();
    let snapshot = cue_cloud_client::TokenStore::load_snapshot(&store)
        .map_err(|e| format!("legacy account keyring unavailable: {e}"))?;
    Ok(snapshot.map(|snapshot| CapturedLegacyKeyringCredentials { store, snapshot }))
}

fn clear_captured_legacy_keyring_credentials(
    captured: Option<CapturedLegacyKeyringCredentials>,
) -> Result<bool, String> {
    let Some(captured) = captured else {
        return Ok(true);
    };
    if cue_cloud_client::TokenStore::clear_if_current(&captured.store, &captured.snapshot)
        .map_err(|e| format!("legacy sign out failed: {e}"))?
    {
        return Ok(true);
    }
    cue_cloud_client::TokenStore::load_snapshot(&captured.store)
        .map(|current| current.is_none())
        .map_err(|e| format!("legacy account keyring unavailable: {e}"))
}

fn clear_captured_dashboard_credentials(
    client: &cue_cloud_client::CloudClient,
    expected: &cue_cloud_client::CredentialSnapshot,
) -> Result<bool, String> {
    client
        .clear_credential_snapshot_if_current(expected)
        .map_err(|e| format!("local sign out failed: {e}"))
}

fn resolve_dashboard_owner(
    account: Option<&cue_core::AccountConfig>,
    tokens: Option<&cue_cloud_client::Tokens>,
) -> Result<DashboardOwner, String> {
    let Some(tokens) = tokens else {
        return Ok(DashboardOwner::Local);
    };
    let Some(account) = account else {
        return Err(owner_identity_error(
            "credentials exist without an account profile",
        ));
    };

    let profile_user_id = account.user_id.trim();
    let credential_user_id = tokens.email.trim();
    if profile_user_id.is_empty()
        || profile_user_id == "local-user"
        || credential_user_id.is_empty()
        || profile_user_id != credential_user_id
    {
        return Err(owner_identity_error(
            "account profile does not match stored credentials",
        ));
    }

    if let Some(cloud_account_id) = account
        .cloud_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return Ok(DashboardOwner::SignedIn(cloud_account_id.to_string()));
    }

    Ok(DashboardOwner::SignedIn(profile_user_id.to_string()))
}

fn owner_identity_error(reason: &str) -> String {
    tracing::warn!(reason, "dashboard account owner identity is unavailable");
    "Signed-in account identity could not be verified. Sign out and sign in again.".to_string()
}

fn current_dashboard_owner_identity() -> Result<DashboardOwnerIdentity, String> {
    let paths = cue_core::app_paths::AppPaths::discover()
        .map_err(|e| format!("account store unavailable: {e}"))?;
    let account =
        cue_core::load_account(&paths).map_err(|e| format!("account profile unavailable: {e}"))?;
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    let tokens = client.current_tokens();
    let owner = resolve_dashboard_owner(account.as_ref(), tokens.as_ref())?;
    Ok(DashboardOwnerIdentity {
        owner,
        credential_generation: account
            .as_ref()
            .map(|account| account.credential_generation)
            .unwrap_or_default(),
    })
}

fn current_dashboard_account_file_stamp() -> Result<DashboardAccountFileStamp, String> {
    let paths = cue_core::app_paths::AppPaths::discover()
        .map_err(|e| format!("account store unavailable: {e}"))?;
    let account =
        cue_core::load_account(&paths).map_err(|e| format!("account profile unavailable: {e}"))?;
    Ok(DashboardAccountFileStamp::from_account(account.as_ref()))
}

/// Resolve credentials only while the lightweight account-profile generation
/// is stable. This closes the gap where another process could replace the
/// profile between the account read and token-store read.
fn current_dashboard_owner_snapshot() -> Result<DashboardOwnerSnapshot, String> {
    for _ in 0..3 {
        let before = current_dashboard_account_file_stamp()?;
        let identity = current_dashboard_owner_identity()?;
        let after = current_dashboard_account_file_stamp()?;
        if before == after && identity.credential_generation == after.credential_generation {
            return Ok(DashboardOwnerSnapshot {
                identity,
                account_file_stamp: after,
            });
        }
    }
    Err(account_changed_message())
}

pub(crate) fn current_dashboard_owner() -> Result<DashboardOwner, String> {
    current_dashboard_owner_snapshot().map(|snapshot| snapshot.identity.owner)
}

fn update_cached_owner(
    cached_owner: &mut Option<DashboardOwner>,
    cached_active: &mut Option<ActiveSessionSelection>,
    next_owner: Option<DashboardOwner>,
    force_clear: bool,
) -> bool {
    let changed = *cached_owner != next_owner;
    if changed || force_clear {
        *cached_owner = next_owner;
        *cached_active = None;
        true
    } else {
        false
    }
}

fn install_dashboard_owner(
    app: &AppHandle,
    owner: Option<DashboardOwner>,
    force_clear: bool,
) -> Result<bool, String> {
    let owner_state = app.state::<DashboardOwnerState>();
    let active_state = app.state::<ActiveSessionState>();
    let reset = {
        let mut cached = owner_state.0.lock().map_err(|e| e.to_string())?;
        let mut cached_active = active_state.0.lock().map_err(|e| e.to_string())?;
        let reset = update_cached_owner(
            &mut cached.owner,
            &mut cached_active,
            owner.clone(),
            force_clear,
        );
        if reset {
            cached.transition_generation = cached.transition_generation.wrapping_add(1);
        }
        cached.transitioning = false;
        reset
    };

    if reset {
        emit_dashboard_owner_change(app, owner.as_ref());
    }
    Ok(reset)
}

fn current_owner_for_app(app: &AppHandle) -> Result<DashboardOwner, String> {
    capture_dashboard_owner_guard(app).map(|guard| guard.identity.owner)
}

pub(crate) fn capture_dashboard_owner_guard(
    app: &AppHandle,
) -> Result<DashboardOwnerGuard, String> {
    let owner_state = app.state::<DashboardOwnerState>();
    let active_state = app.state::<ActiveSessionState>();
    let (resolved, reset) = {
        let mut cached = owner_state.0.lock().map_err(|e| e.to_string())?;
        if cached.transitioning {
            return Err("Account change in progress. Try again in a moment.".to_string());
        }
        let resolved = current_dashboard_owner_snapshot();
        let next_owner = resolved
            .as_ref()
            .ok()
            .map(|snapshot| snapshot.identity.owner.clone());
        let mut cached_active = active_state.0.lock().map_err(|e| e.to_string())?;
        let reset = update_cached_owner(&mut cached.owner, &mut cached_active, next_owner, false);
        if reset {
            cached.transition_generation = cached.transition_generation.wrapping_add(1);
        }
        let guard = resolved.map(|snapshot| DashboardOwnerGuard {
            identity: snapshot.identity,
            account_file_stamp: snapshot.account_file_stamp,
            transition_generation: cached.transition_generation,
        });
        (guard, reset)
    };
    if reset {
        emit_dashboard_owner_change(
            app,
            resolved.as_ref().ok().map(|guard| &guard.identity.owner),
        );
    }
    resolved
}

impl DashboardOwnerGuard {
    pub(crate) fn mutation_fence(
        &self,
        meeting_id: Option<Uuid>,
        audio_session_id: Option<String>,
    ) -> DaemonMutationFence {
        DaemonMutationFence {
            owner_account_id: self.identity.owner.db_owner_id().map(str::to_string),
            credential_generation: self
                .identity
                .owner
                .signed_in()
                .then_some(self.identity.credential_generation),
            meeting_id,
            audio_session_id,
            capture_generation: None,
        }
    }

    fn validate(
        &self,
        cache: &DashboardOwnerCache,
        current: &DashboardOwnerSnapshot,
    ) -> Result<(), String> {
        if cache.transitioning
            || cache.transition_generation != self.transition_generation
            || cache.owner.as_ref() != Some(&self.identity.owner)
            || current.identity != self.identity
            || current.account_file_stamp != self.account_file_stamp
        {
            return Err(account_changed_message());
        }
        Ok(())
    }

    fn validate_cached_epoch(&self, cache: &DashboardOwnerCache) -> Result<(), String> {
        if cache.transitioning
            || cache.transition_generation != self.transition_generation
            || cache.owner.as_ref() != Some(&self.identity.owner)
        {
            return Err(account_changed_message());
        }
        Ok(())
    }

    fn validate_account_file_stamp(
        &self,
        current: &DashboardAccountFileStamp,
    ) -> Result<(), String> {
        if current != &self.account_file_stamp {
            return Err(account_changed_message());
        }
        Ok(())
    }

    fn with_current<T>(
        &self,
        app: &AppHandle,
        action: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let owner_state = app.state::<DashboardOwnerState>();
        let cache = owner_state.0.lock().map_err(|e| e.to_string())?;
        let current = current_dashboard_owner_snapshot()?;
        self.validate(&cache, &current)?;
        let result = action()?;
        let current = current_dashboard_owner_snapshot()?;
        self.validate(&cache, &current)?;
        Ok(result)
    }

    fn ensure_current(&self, app: &AppHandle) -> Result<(), String> {
        self.with_current(app, || Ok(()))
    }

    pub(crate) fn begin_transition_if_current(&self, app: &AppHandle) -> Result<(), String> {
        let owner_state = app.state::<DashboardOwnerState>();
        let active_state = app.state::<ActiveSessionState>();
        let reset = {
            let mut cached = owner_state.0.lock().map_err(|e| e.to_string())?;
            let current = current_dashboard_owner_snapshot()?;
            self.validate(&cached, &current)?;
            let mut cached_active = active_state.0.lock().map_err(|e| e.to_string())?;
            let reset = update_cached_owner(&mut cached.owner, &mut cached_active, None, true);
            cached.transitioning = true;
            cached.transition_generation = cached.transition_generation.wrapping_add(1);
            reset
        };
        if reset {
            emit_dashboard_owner_change(app, None);
        }
        Ok(())
    }
}

impl DashboardOwnerStreamFence {
    /// Start one bounded account-profile monitor for an answer stream. The
    /// monitor performs the cross-process file reads; per-chunk checks use
    /// only the watch bit plus the in-process owner epoch.
    fn start(app: &AppHandle, owner_guard: &DashboardOwnerGuard) -> Result<Self, String> {
        owner_guard.ensure_current(app)?;
        owner_guard.validate_account_file_stamp(&current_dashboard_account_file_stamp()?)?;

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let stop = Arc::new(AtomicBool::new(false));
        let monitor_stop = stop.clone();
        let monitor_app = app.clone();
        let monitor_guard = owner_guard.clone();
        let monitor = tokio::task::spawn_blocking(move || {
            while !monitor_stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(20));
                if monitor_stop.load(Ordering::Acquire) {
                    break;
                }

                let cache_is_current = monitor_app
                    .state::<DashboardOwnerState>()
                    .0
                    .lock()
                    .ok()
                    .is_some_and(|cache| monitor_guard.validate_cached_epoch(&cache).is_ok());
                let profile_is_current = current_dashboard_account_file_stamp()
                    .and_then(|stamp| monitor_guard.validate_account_file_stamp(&stamp))
                    .is_ok();
                if !cache_is_current || !profile_is_current {
                    let _ = cancel_tx.send(true);
                    break;
                }
            }
        });

        Ok(Self {
            token: DashboardOwnerStreamToken {
                cancelled: cancel_rx,
            },
            stop,
            _monitor: monitor,
        })
    }

    fn token(&self) -> DashboardOwnerStreamToken {
        self.token.clone()
    }
}

impl Drop for DashboardOwnerStreamFence {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl DashboardOwnerStreamToken {
    fn receiver(&self) -> watch::Receiver<bool> {
        self.cancelled.clone()
    }

    fn with_current<T>(
        &self,
        owner_guard: &DashboardOwnerGuard,
        app: &AppHandle,
        action: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let owner_state = app.state::<DashboardOwnerState>();
        let cache = owner_state.0.lock().map_err(|e| e.to_string())?;
        self.validate_fast(owner_guard, &cache)?;
        action()
    }

    fn validate_fast(
        &self,
        owner_guard: &DashboardOwnerGuard,
        cache: &DashboardOwnerCache,
    ) -> Result<(), String> {
        if *self.cancelled.borrow() {
            return Err(account_changed_message());
        }
        owner_guard.validate_cached_epoch(cache)
    }
}

async fn wait_for_owner_stream_cancellation(cancelled: &mut watch::Receiver<bool>) {
    if *cancelled.borrow() {
        return;
    }
    while cancelled.changed().await.is_ok() {
        if *cancelled.borrow() {
            return;
        }
    }
}

fn dashboard_mutation_fence(
    app: &AppHandle,
    owner_guard: &DashboardOwnerGuard,
    audio_session_id: Option<String>,
) -> Result<DaemonMutationFence, String> {
    owner_guard.with_current(app, || {
        let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
        let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;
        let meeting = store.load_active().map_err(|e| e.to_string())?;
        if meeting.as_ref().is_some_and(|meeting| {
            !owner_guard
                .identity
                .owner
                .owns_meeting(meeting.owner_account_id.as_deref())
        }) {
            return Err("The active session belongs to another account.".to_string());
        }
        Ok(owner_guard.mutation_fence(meeting.map(|meeting| meeting.id), audio_session_id))
    })
}

fn emit_dashboard_owner_change(app: &AppHandle, owner: Option<&DashboardOwner>) {
    let payload = owner
        .map(DashboardOwnerPayload::available)
        .unwrap_or_else(DashboardOwnerPayload::unavailable);
    if let Err(error) = app.emit("dashboard_owner_changed", payload) {
        tracing::warn!(%error, "failed to emit dashboard_owner_changed event");
    }
    if let Err(error) = app.emit("session:switched", SessionSwitchedPayload { id: None }) {
        tracing::warn!(%error, "failed to emit session:switched event for owner change");
    }
}

fn suspend_dashboard_owner(app: &AppHandle) -> Result<bool, String> {
    let owner_state = app.state::<DashboardOwnerState>();
    let active_state = app.state::<ActiveSessionState>();
    let reset = {
        let mut cached = owner_state.0.lock().map_err(|e| e.to_string())?;
        let mut cached_active = active_state.0.lock().map_err(|e| e.to_string())?;
        let reset = update_cached_owner(&mut cached.owner, &mut cached_active, None, true);
        cached.transitioning = true;
        cached.transition_generation = cached.transition_generation.wrapping_add(1);
        reset
    };
    if reset {
        emit_dashboard_owner_change(app, None);
    }
    Ok(reset)
}

impl DashboardOwnerCache {
    pub(crate) fn new(owner: Option<DashboardOwner>) -> Self {
        Self {
            owner,
            transitioning: false,
            transition_generation: 0,
        }
    }
}

pub(crate) fn refresh_dashboard_owner_after_account_change(
    app: &AppHandle,
) -> Result<DashboardOwnerPayload, String> {
    match current_dashboard_owner() {
        Ok(owner) => {
            install_dashboard_owner(app, Some(owner.clone()), true)?;
            Ok(DashboardOwnerPayload::available(&owner))
        }
        Err(error) => {
            let _ = install_dashboard_owner(app, None, true);
            Err(error)
        }
    }
}

pub(crate) fn begin_dashboard_owner_change(app: &AppHandle) -> Result<(), String> {
    suspend_dashboard_owner(app).map(|_| ())
}

#[tauri::command]
pub fn get_dashboard_owner(app: AppHandle) -> Result<DashboardOwnerPayload, String> {
    let owner = current_owner_for_app(&app)?;
    Ok(DashboardOwnerPayload::available(&owner))
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub async fn get_balance_snapshot(
    app: AppHandle,
) -> Result<Option<BalanceSnapshotPayload>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    let Some(credentials) = client.credential_snapshot() else {
        owner_guard.ensure_current(&app)?;
        return Ok(None);
    };
    let legacy_credentials = capture_legacy_keyring_credentials_if_enabled()?;

    let me: cue_cloud_client::AccountMe = match async {
        verify_dashboard_device_link(&client, &credentials).await?;
        client.auth_get("/account/me").await
    }
    .await
    {
        Ok(me) => {
            owner_guard.ensure_current(&app)?;
            me
        }
        Err(error) if dashboard_auth_error_should_clear_tokens(&error) => {
            clear_revoked_dashboard_account(
                &app,
                &trace_id,
                &owner_guard,
                &client,
                &credentials,
                legacy_credentials,
            )
            .await?;
            return Ok(None);
        }
        Err(error) => {
            owner_guard.ensure_current(&app)?;
            return Err(format!("balance lookup failed: {error}"));
        }
    };
    Ok(Some(BalanceSnapshotPayload {
        balance_cents: me.balance_cents,
        balance_label: format_cents(me.balance_cents),
        trial_seconds_remaining: me.trial_seconds_remaining,
        auto_topup_enabled: me.auto_topup_enabled,
        auto_topup_threshold_cents: me.auto_topup_threshold_cents,
        auto_topup_amount_cents: me.auto_topup_amount_cents,
        low_balance_warning: me.balance_cents < me.auto_topup_threshold_cents
            && me.balance_cents > 0,
    }))
}

#[tauri::command]
pub async fn account_me(app: AppHandle) -> Result<Option<AccountMePayload>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    let Some(credentials) = client.credential_snapshot() else {
        owner_guard.ensure_current(&app)?;
        return Ok(None);
    };
    let legacy_credentials = capture_legacy_keyring_credentials_if_enabled()?;

    let me: cue_cloud_client::AccountMe = match async {
        verify_dashboard_device_link(&client, &credentials).await?;
        client.auth_get("/account/me").await
    }
    .await
    {
        Ok(me) => {
            owner_guard.ensure_current(&app)?;
            me
        }
        Err(error) if dashboard_auth_error_should_clear_tokens(&error) => {
            clear_revoked_dashboard_account(
                &app,
                &trace_id,
                &owner_guard,
                &client,
                &credentials,
                legacy_credentials,
            )
            .await?;
            return Ok(None);
        }
        Err(error) => {
            owner_guard.ensure_current(&app)?;
            return Err(format!("account lookup failed: {error}"));
        }
    };
    Ok(Some(AccountMePayload {
        id: me.id,
        email: me.email,
        balance_cents: me.balance_cents,
        trial_seconds_remaining: me.trial_seconds_remaining,
        auto_topup_enabled: me.auto_topup_enabled,
        auto_topup_threshold_cents: me.auto_topup_threshold_cents,
        auto_topup_amount_cents: me.auto_topup_amount_cents,
    }))
}

async fn verify_dashboard_device_link(
    client: &cue_cloud_client::CloudClient,
    credentials: &cue_cloud_client::CredentialSnapshot,
) -> Result<(), cue_cloud_client::Error> {
    let Some(device_id) = credentials.authority().device_id() else {
        return Ok(());
    };
    let status: cue_cloud_client::DeviceStatusResponse = client
        .auth_post(
            "/account/devices/status",
            &cue_cloud_client::DeviceStatusRequest {
                device_id: device_id.to_string(),
            },
        )
        .await?;
    if status.active {
        Ok(())
    } else {
        Err(cue_cloud_client::Error::Unauthorized)
    }
}

fn dashboard_auth_error_should_clear_tokens(error: &cue_cloud_client::Error) -> bool {
    matches!(
        error,
        cue_cloud_client::Error::Unauthorized
            | cue_cloud_client::Error::Server { status: 403 | 404 }
    )
}

async fn clear_revoked_dashboard_account(
    app: &AppHandle,
    trace_id: &str,
    owner_guard: &DashboardOwnerGuard,
    client: &cue_cloud_client::CloudClient,
    credentials: &cue_cloud_client::CredentialSnapshot,
    legacy_credentials: Option<CapturedLegacyKeyringCredentials>,
) -> Result<(), String> {
    owner_guard.begin_transition_if_current(app)?;
    let daemon_committed = matches!(
        daemon_ipc_with_trace(
            DaemonRequest::CloudLogoutBound {
                fence: owner_guard.mutation_fence(None, None),
            },
            trace_id,
        )
        .await,
        Ok(response) if !matches!(response, DaemonResponse::Error { .. })
    );
    if !daemon_committed {
        tracing::warn!(
            error_category = "daemon_signout_commit_failed",
            "daemon cleanup failed after dashboard auth revocation"
        );
    }
    if !clear_captured_dashboard_credentials(client, credentials)? {
        tracing::info!("revoked dashboard credential snapshot was already cleared or replaced");
    }
    if !clear_captured_legacy_keyring_credentials(legacy_credentials)? {
        tracing::warn!(
            "revoked legacy dashboard credentials changed while cleanup was pending; newer credentials were kept"
        );
    }
    if let Err(error) = refresh_dashboard_owner_after_account_change(app) {
        tracing::warn!(%error, "failed to refresh dashboard owner after auth revocation");
    }
    Ok(())
}

#[tauri::command]
pub async fn billing_portal_url(app: AppHandle) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct PortalResponse {
        portal_url: String,
    }

    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let client = cloud_client_with_trace(&trace_id)?;
    let credentials = client
        .credential_snapshot()
        .ok_or_else(|| "Sign in again before opening billing.".to_string())?;
    let legacy_credentials = capture_legacy_keyring_credentials_if_enabled()?;
    let resp: PortalResponse = match client
        .auth_post("/billing/portal", &serde_json::json!({}))
        .await
    {
        Ok(response) => {
            owner_guard.ensure_current(&app)?;
            response
        }
        Err(error) if dashboard_auth_error_should_clear_tokens(&error) => {
            clear_revoked_dashboard_account(
                &app,
                &trace_id,
                &owner_guard,
                &client,
                &credentials,
                legacy_credentials,
            )
            .await?;
            return Err("Sign in again before opening billing.".to_string());
        }
        Err(error) => {
            owner_guard.ensure_current(&app)?;
            return Err(format!("billing portal failed: {error}"));
        }
    };
    Ok(resp.portal_url)
}

#[tauri::command]
pub async fn sign_out(db: State<'_, DbState>, app: AppHandle) -> Result<(), String> {
    let trace_id = dashboard_trace_id();
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let client = cloud_client_with_trace(&trace_id)?;
    let credentials = client.credential_snapshot();
    let legacy_credentials = capture_legacy_keyring_credentials_if_enabled()?;
    begin_dashboard_owner_change(&app)?;
    let daemon_signed_out = if credentials.is_some() {
        matches!(
            daemon_ipc_with_trace(
                DaemonRequest::CloudLogoutBound {
                    fence: owner_guard.mutation_fence(None, None),
                },
                &trace_id,
            )
            .await,
            Ok(response) if !matches!(response, DaemonResponse::Error { .. })
        )
    } else {
        true
    };
    let _current_credentials_cleared = match credentials.as_ref() {
        Some(credentials) => clear_captured_dashboard_credentials(&client, credentials)?,
        None => true,
    };
    let legacy_credentials_cleared = clear_captured_legacy_keyring_credentials(legacy_credentials)?;
    let current_owner = refresh_dashboard_owner_after_account_change(&app)?;
    if current_owner.signed_in || !legacy_credentials_cleared {
        tracing::warn!(
            error_category = "signout_credentials_replaced",
            "sign out preserved credentials that changed while cleanup was pending"
        );
        return Err(account_changed_message());
    }
    if !daemon_signed_out {
        tracing::warn!(
            error_category = "daemon_signout_commit_failed",
            "daemon-owned account sign-out did not commit"
        );
        return Err("Bluey couldn't finish signing out on this computer.".to_string());
    }
    if mark_onboarding_incomplete(db).is_err() {
        let _ = refresh_dashboard_owner_after_account_change(&app);
        tracing::warn!(
            error_category = "onboarding_reset_failed",
            "local sign out did not reset onboarding state"
        );
        return Err("Bluey couldn't finish signing out on this computer.".to_string());
    }
    refresh_dashboard_owner_after_account_change(&app)?;
    Ok(())
}

#[tauri::command]
pub async fn delete_account_now(app: AppHandle) -> Result<(), String> {
    let trace_id = dashboard_trace_id();
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let account_context = DashboardAccountCloudContext::snapshot(&trace_id).map_err(|_| {
        tracing::warn!(
            error_category = "account_snapshot_unavailable",
            "account deletion could not capture its local account snapshot"
        );
        "Bluey could not safely prepare account deletion on this computer.".to_string()
    })?;
    if owner_guard.identity.owner.db_owner_id() != Some(account_context.owner_account_id.as_str())
        || owner_guard.identity.credential_generation != account_context.credential_generation
    {
        return Err(account_changed_message());
    }
    let owner_account_id = account_context.owner_account_id.clone();
    let proposed = cue_cloud_client::types::AccountDeletionCapability::new();
    let prepared = daemon_ipc_with_trace(
        DaemonRequest::CloudPrepareAccountDeletion {
            owner_account_id: owner_account_id.clone(),
            operation_id: proposed.operation_id,
            recovery_token: proposed.recovery_token,
        },
        &trace_id,
    )
    .await
    .map_err(|_| {
        tracing::warn!(
            error_category = "daemon_prepare_unavailable",
            "account deletion local fence request failed"
        );
        "Bluey could not safely prepare local account cleanup.".to_string()
    })?;
    let capability = match prepared {
        DaemonResponse::AccountDeletionPrepared {
            operation_id,
            recovery_token,
        } => cue_cloud_client::types::AccountDeletionCapability {
            operation_id,
            recovery_token,
        },
        DaemonResponse::Error { .. } => {
            tracing::warn!(
                error_category = "daemon_prepare_rejected",
                "account deletion local fence was rejected"
            );
            return Err("Bluey could not safely prepare local account cleanup.".to_string());
        }
        _ => return Err("Bluey could not verify its local deletion marker.".to_string()),
    };
    account_context.ensure_current()?;
    owner_guard.begin_transition_if_current(&app).map_err(|_| {
        tracing::warn!(
            error_category = "owner_transition_failed",
            "account deletion could not publish its local owner fence"
        );
        "Bluey could not safely begin account deletion on this computer.".to_string()
    })?;
    account_context.ensure_current()?;
    let ack = match account_context.client.delete_account(&capability).await {
        Ok(ack) => ack,
        Err(_) => match account_context
            .client
            .account_deletion_status(&capability)
            .await
        {
            Ok(status) if status.deleted => cue_cloud_client::types::AccountDeletionAck {
                deleted: true,
                deleted_at: status.deleted_at,
                deletion_pending: false,
                retry_after_ms: None,
                object_count_deleted: 0,
                note: None,
            },
            Ok(status) if status.deletion_pending => {
                tracing::warn!(
                    error_category = "server_deletion_pending",
                    "account deletion is durably pending; local fence retained"
                );
                return Err(
                    "Account deletion is safely in progress. Local account writes remain paused; try again shortly."
                        .to_string(),
                );
            }
            Ok(_) | Err(_) => {
                tracing::warn!(
                    error_category = "server_result_unknown",
                    "account deletion result is unknown; local fence retained"
                );
                return Err(
                    "Bluey could not confirm the server result. Local account writes remain paused; try deletion again to reconcile safely."
                        .to_string(),
                );
            }
        },
    };
    if ack.deleted {
        let cleanup = daemon_ipc_with_trace(
            DaemonRequest::CloudPurgeDeletedAccount {
                owner_account_id: owner_account_id.clone(),
            },
            &trace_id,
        )
        .await;
        let cleanup_failed = !matches!(
            cleanup,
            Ok(response) if !matches!(response, DaemonResponse::Error { .. })
        );
        if cleanup_failed {
            let _ = refresh_dashboard_owner_after_account_change(&app);
            tracing::error!(
                error_category = "daemon_local_purge_failed",
                "local account purge incomplete after server deletion"
            );
            return Err(
                "Your Bluey account was deleted, but this computer could not finish removing its local copy. Restart Bluey to resume the protected local cleanup before signing in again."
                    .to_string(),
            );
        }
        // The daemon owns data, profile-token generation, and onboarding
        // cleanup under the same answer-persistence barrier. The dashboard
        // publishes its local owner only after that transaction completes.
        install_dashboard_owner(&app, Some(DashboardOwner::Local), true).map_err(|_| {
            tracing::error!(
                error_category = "owner_cleanup_failed",
                "account deleted but local owner cleanup failed"
            );
            "Your account was deleted, but Bluey could not finish local cleanup.".to_string()
        })?;
        let acknowledged = daemon_ipc_with_trace(
            DaemonRequest::CloudAcknowledgeDeletedAccountPurge {
                owner_account_id: owner_account_id.clone(),
                operation_id: capability.operation_id.clone(),
                recovery_token: capability.recovery_token.clone(),
            },
            &trace_id,
        )
        .await;
        if !matches!(acknowledged, Ok(response) if !matches!(response, DaemonResponse::Error { .. }))
        {
            tracing::error!(
                error_category = "local_cleanup_ack_failed",
                "deleted-account cleanup marker remains for restart-safe acknowledgement"
            );
            return Err(
                "Your account was deleted and local data was removed, but Bluey still needs to confirm cleanup. Restart Bluey before signing in again."
                    .to_string(),
            );
        }
    } else {
        if ack.deletion_pending {
            return Err(ack.note.unwrap_or_else(|| {
                "Deletion is safely fenced and waiting for an in-flight upload. Try again shortly."
                    .to_string()
            }));
        }
        let abort = daemon_ipc_with_trace(
            DaemonRequest::CloudAbortAccountDeletion { owner_account_id },
            &trace_id,
        )
        .await;
        if !matches!(abort, Ok(response) if !matches!(response, DaemonResponse::Error { .. })) {
            tracing::error!(
                error_category = "local_fence_abort_failed",
                "server declined account deletion but local fence cleanup failed"
            );
        }
        let _ = refresh_dashboard_owner_after_account_change(&app);
        return Err(ack.note.unwrap_or_else(|| {
            "The server did not delete this account. Local activity has resumed.".to_string()
        }));
    }
    Ok(())
}

#[tauri::command]
pub fn report_frontend_error(payload: FrontendErrorPayload) -> Result<(), String> {
    let command = payload
        .command
        .as_deref()
        .filter(|value| is_safe_frontend_command(value))
        .unwrap_or("other");
    tracing::warn!(
        source = ?payload.source,
        category = ?payload.category,
        command,
        "frontend error captured"
    );
    Ok(())
}

fn is_safe_frontend_command(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[tauri::command]
pub fn get_signin_url() -> String {
    std::env::var("BLUEY_SIGNIN_URL").unwrap_or_else(|_| "https://bluey.sh/login".to_string())
}

#[tauri::command]
pub fn complete_onboarding(db: State<DbState>, app: AppHandle) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.save_setting("onboarding_complete", "true")
            .map_err(|e| e.to_string())
    })
}

fn mark_onboarding_incomplete(db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.save_setting("onboarding_complete", "false")
        .map_err(|e| e.to_string())
}

fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.saturating_abs();
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

fn dashboard_list_sessions(
    db: &cue_daemon::db::Database,
    owner: &DashboardOwner,
) -> Result<Vec<Session>, String> {
    db.list_sessions_for_owner(owner.db_owner_id(), None, 100)
        .map_err(|e| e.to_string())
}

fn dashboard_get_session(
    db: &cue_daemon::db::Database,
    owner: &DashboardOwner,
    id: Uuid,
) -> Result<Option<Session>, String> {
    db.get_session_for_owner(owner.db_owner_id(), id)
        .map_err(|e| e.to_string())
}

fn active_session_for_owner(
    db: &cue_daemon::db::Database,
    active: &ActiveSessionState,
    owner: &DashboardOwner,
) -> Result<Option<Uuid>, String> {
    {
        let cached = active.0.lock().map_err(|e| e.to_string())?;
        if let Some(selection) = cached
            .as_ref()
            .filter(|selection| &selection.owner == owner)
        {
            return Ok(selection.id);
        }
    }

    let id = db
        .load_active_session_for_owner(owner.db_owner_id())
        .map_err(|e| e.to_string())?;
    let mut cached = active.0.lock().map_err(|e| e.to_string())?;
    *cached = Some(ActiveSessionSelection {
        owner: owner.clone(),
        id,
    });
    Ok(id)
}

fn cache_active_session(
    active: &ActiveSessionState,
    owner: &DashboardOwner,
    id: Option<Uuid>,
) -> Result<(), String> {
    let mut cached = active.0.lock().map_err(|e| e.to_string())?;
    *cached = Some(ActiveSessionSelection {
        owner: owner.clone(),
        id,
    });
    Ok(())
}

fn daemon_session_owner_matches(owner: &DashboardOwner, session: &DaemonSessionRecord) -> bool {
    let record_owner = session
        .owner_account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    owner.db_owner_id() == record_owner
}

fn lifecycle_timestamp(value: &str) -> i64 {
    value.trim().parse::<i64>().unwrap_or_default()
}

fn dashboard_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn project_daemon_session(
    db: &cue_daemon::db::Database,
    owner: &DashboardOwner,
    session: &DaemonSessionRecord,
) -> Result<Session, String> {
    if !daemon_session_owner_matches(owner, session) {
        return Err(format!(
            "daemon session {} does not belong to the current account",
            session.id
        ));
    }
    let created_at = lifecycle_timestamp(&session.started_at);
    let updated_at = session
        .ended_at
        .as_deref()
        .map(lifecycle_timestamp)
        .unwrap_or_else(dashboard_now_ms)
        .max(created_at);
    db.ensure_session_record_for_owner(
        owner.db_owner_id(),
        session.id,
        &session.title,
        created_at,
        updated_at,
    )
    .map_err(|error| error.to_string())?;
    let status = if session.active {
        SessionStatus::Active
    } else if session.ended_at.is_some() {
        SessionStatus::Archived
    } else {
        SessionStatus::Paused
    };
    db.update_session_status_for_owner(owner.db_owner_id(), session.id, status)
        .map_err(|error| error.to_string())?;
    dashboard_get_session(db, owner, session.id)?
        .ok_or_else(|| format!("session {} projection is missing", session.id))
}

fn project_session_lifecycle(
    db: &cue_daemon::db::Database,
    owner: &DashboardOwner,
    lifecycle: &DaemonSessionLifecycle,
) -> Result<Option<Session>, String> {
    for session in [
        lifecycle.changed.as_ref(),
        lifecycle.replaced.as_ref(),
        lifecycle.deleted.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !daemon_session_owner_matches(owner, session) {
            return Err(format!(
                "daemon session {} does not belong to the current account",
                session.id
            ));
        }
    }

    if let Some(replaced) = lifecycle.replaced.as_ref() {
        project_daemon_session(db, owner, replaced)?;
    }
    let changed = lifecycle
        .changed
        .as_ref()
        .map(|session| project_daemon_session(db, owner, session))
        .transpose()?;
    if let Some(deleted) = lifecycle.deleted.as_ref() {
        db.delete_session_for_owner(owner.db_owner_id(), deleted.id)
            .map_err(|error| error.to_string())?;
    }
    if let Some(active_id) = lifecycle.active_session_id {
        if dashboard_get_session(db, owner, active_id)?.is_none() {
            return Err(format!(
                "daemon active session {active_id} is missing from the dashboard projection"
            ));
        }
    }
    db.save_active_session_for_owner(owner.db_owner_id(), lifecycle.active_session_id)
        .map_err(|error| error.to_string())?;
    Ok(changed)
}

fn session_lifecycle_from_response(
    response: DaemonResponse,
) -> Result<DaemonSessionLifecycle, String> {
    match response {
        DaemonResponse::SessionLifecycle { lifecycle } => Ok(lifecycle),
        DaemonResponse::Error { message } => Err(message),
        _ => Err("Bluey's local service returned an unexpected session response.".to_string()),
    }
}

#[tauri::command]
pub fn list_sessions(db: State<DbState>, app: AppHandle) -> Result<Vec<Session>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        dashboard_list_sessions(&db, &owner_guard.identity.owner)
    })
}

#[tauri::command]
pub async fn create_session(
    title: Option<String>,
    db: State<'_, DbState>,
    active: State<'_, ActiveSessionState>,
    app: AppHandle,
) -> Result<Session, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let lifecycle = session_lifecycle_from_response(
        daemon_ipc(DaemonRequest::SessionCreateBound { title, fence }).await?,
    )?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        let session = project_session_lifecycle(&db, &owner_guard.identity.owner, &lifecycle)?
            .ok_or_else(|| "daemon did not return the created session".to_string())?;
        drop(db);
        cache_active_session(
            &active,
            &owner_guard.identity.owner,
            lifecycle.active_session_id,
        )?;
        if let Err(e) = app.emit("session:created", &session) {
            tracing::warn!(error = %e, "failed to emit session:created event");
        }
        if let Err(e) = app.emit(
            "session:switched",
            SessionSwitchedPayload {
                id: lifecycle.active_session_id.map(|id| id.to_string()),
            },
        ) {
            tracing::warn!(error = %e, "failed to emit session:switched event");
        }
        Ok(session)
    })
}

#[tauri::command]
pub fn get_session(
    id: String,
    db: State<DbState>,
    app: AppHandle,
) -> Result<Option<Session>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        dashboard_get_session(&db, &owner_guard.identity.owner, uuid)
    })
}

#[tauri::command]
pub async fn archive_session(
    id: String,
    db: State<'_, DbState>,
    active: State<'_, ActiveSessionState>,
    app: AppHandle,
) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let previous = owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        active_session_for_owner(&db, &active, &owner_guard.identity.owner)
    })?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let lifecycle = session_lifecycle_from_response(
        daemon_ipc(DaemonRequest::SessionArchiveBound { id: uuid, fence }).await?,
    )?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        project_session_lifecycle(&db, &owner_guard.identity.owner, &lifecycle)?;
        drop(db);
        cache_active_session(
            &active,
            &owner_guard.identity.owner,
            lifecycle.active_session_id,
        )?;
        if previous != lifecycle.active_session_id {
            let _ = app.emit(
                "session:switched",
                SessionSwitchedPayload {
                    id: lifecycle.active_session_id.map(|id| id.to_string()),
                },
            );
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn delete_session(
    id: String,
    db: State<'_, DbState>,
    active: State<'_, ActiveSessionState>,
    app: AppHandle,
) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let previous = owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        active_session_for_owner(&db, &active, &owner_guard.identity.owner)
    })?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let lifecycle = session_lifecycle_from_response(
        daemon_ipc(DaemonRequest::SessionDeleteBound { id: uuid, fence }).await?,
    )?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        project_session_lifecycle(&db, &owner_guard.identity.owner, &lifecycle)?;
        drop(db);
        cache_active_session(
            &active,
            &owner_guard.identity.owner,
            lifecycle.active_session_id,
        )?;
        if previous != lifecycle.active_session_id {
            if let Err(e) = app.emit(
                "session:switched",
                SessionSwitchedPayload {
                    id: lifecycle.active_session_id.map(|id| id.to_string()),
                },
            ) {
                tracing::warn!(error = %e, "failed to emit session:switched event");
            }
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn update_session_title(
    id: String,
    title: String,
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let uuid = Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let lifecycle = session_lifecycle_from_response(
        daemon_ipc(DaemonRequest::SessionRenameBound {
            id: uuid,
            title,
            fence,
        })
        .await?,
    )?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        project_session_lifecycle(&db, &owner_guard.identity.owner, &lifecycle)?;
        Ok(())
    })
}

/// Return the currently-active session id, or `None` if no session is selected.
#[tauri::command]
pub fn get_active_session(
    db: State<DbState>,
    active: State<ActiveSessionState>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        Ok(
            active_session_for_owner(&db, &active, &owner_guard.identity.owner)?
                .map(|id| id.to_string()),
        )
    })
}

/// Set the active session. Pass `None` to clear the selection.
///
/// Validates that the target session exists before switching (avoids pointing
/// at an id that was just deleted in another window). Emits `session:switched`
/// whenever the selection changes.
#[tauri::command]
pub async fn set_active_session(
    id: Option<String>,
    db: State<'_, DbState>,
    active: State<'_, ActiveSessionState>,
    app: AppHandle,
) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let previous = owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        active_session_for_owner(&db, &active, &owner_guard.identity.owner)
    })?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let request = match id {
        Some(raw) => DaemonRequest::SessionActivateBound {
            id: Uuid::parse_str(&raw).map_err(|e| e.to_string())?,
            fence,
        },
        None => DaemonRequest::SessionDeactivateBound { fence },
    };
    let lifecycle = session_lifecycle_from_response(daemon_ipc(request).await?)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        project_session_lifecycle(&db, &owner_guard.identity.owner, &lifecycle)?;
        drop(db);
        let new_id = lifecycle.active_session_id;
        let changed = previous != new_id;
        cache_active_session(&active, &owner_guard.identity.owner, new_id)?;

        if changed {
            let payload = SessionSwitchedPayload {
                id: new_id.map(|u| u.to_string()),
            };
            if let Err(e) = app.emit("session:switched", payload) {
                tracing::warn!(error = %e, "failed to emit session:switched event");
            }
        }
        Ok(())
    })
}

/// List turns for a session (read-only). Used by the session detail page.
#[tauri::command]
pub fn list_turns(
    session_id: String,
    db: State<DbState>,
    app: AppHandle,
) -> Result<Vec<cue_core::session::Turn>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let uuid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.list_turns_for_owner(owner_guard.identity.owner.db_owner_id(), uuid, None)
            .map_err(|e| e.to_string())
    })
}

// ===== Settings + Secrets commands (Phase 3 Round 5) =====

const SUPPORT_CLEANUP_OWNER_MESSAGE: &str = concat!(
    "Support diagnostic cleanup belongs to another or unverified account. ",
    "Sign in to the original account to finish cleanup."
);

#[derive(Clone, Serialize)]
pub struct DataControlsPayload {
    pub account_scope_available: bool,
    pub cloud_sync_enabled: bool,
    pub support_diagnostics_upload_enabled: bool,
    pub support_diagnostics_server_cleanup_pending: bool,
    pub support_diagnostics_cleanup_waiting_for_another_account: bool,
    pub raw_audio_retained: bool,
    pub training_enabled: bool,
}

fn data_controls_payload(
    settings: &cue_core::CueSettings,
    owner: &DashboardOwner,
) -> DataControlsPayload {
    let account_id = owner.db_owner_id();
    DataControlsPayload {
        account_scope_available: account_id.is_some(),
        cloud_sync_enabled: settings.cloud_sync_allowed_for_account(account_id),
        support_diagnostics_upload_enabled: settings
            .support_diagnostics_upload_allowed_for_account(account_id),
        support_diagnostics_server_cleanup_pending: settings
            .support_diagnostics_revocation_pending_for_account(account_id),
        support_diagnostics_cleanup_waiting_for_another_account: settings
            .support_diagnostics_revocation_waits_for_other_account(account_id),
        raw_audio_retained: false,
        training_enabled: false,
    }
}

fn set_cloud_sync_for_account(
    settings: &mut cue_core::CueSettings,
    owner_account_id: &str,
    enabled: bool,
) {
    if enabled {
        settings.cloud_sync_consent_account_id = Some(owner_account_id.to_string());
        settings.cloud_sync_consent_granted = true;
        settings.cloud_sync_enabled = true;
    } else if settings.cloud_sync_consent_scoped_to(Some(owner_account_id)) {
        settings.cloud_sync_enabled = false;
        settings.cloud_sync_consent_granted = false;
        settings.cloud_sync_consent_account_id = None;
    }
}

fn begin_support_diagnostics_revocation(
    settings: &mut cue_core::CueSettings,
    owner_account_id: &str,
) -> Result<bool, String> {
    if settings.support_diagnostics_revocation_waits_for_other_account(Some(owner_account_id)) {
        return Err(SUPPORT_CLEANUP_OWNER_MESSAGE.to_string());
    }
    if !settings.support_diagnostics_consent_scoped_to(Some(owner_account_id))
        && !settings.support_diagnostics_revocation_pending_for_account(Some(owner_account_id))
    {
        return Ok(false);
    }
    settings.support_diagnostics_upload_enabled = false;
    settings.support_diagnostics_upload_consent_granted = false;
    settings.support_diagnostics_upload_consent_account_id = None;
    settings.support_diagnostics_server_revocation_pending = true;
    settings.support_diagnostics_server_revocation_account_id = Some(owner_account_id.to_string());
    Ok(true)
}

fn complete_support_diagnostics_revocation(
    settings: &mut cue_core::CueSettings,
    owner_account_id: &str,
) {
    if settings.support_diagnostics_revocation_pending_for_account(Some(owner_account_id)) {
        settings.support_diagnostics_server_revocation_pending = false;
        settings.support_diagnostics_server_revocation_account_id = None;
        settings.support_diagnostics_upload_enabled = false;
        settings.support_diagnostics_upload_consent_granted = false;
        settings.support_diagnostics_upload_consent_account_id = None;
    }
}

async fn compensate_support_diagnostic_grant(
    account_context: &DashboardAccountCloudContext,
    paths: &cue_core::app_paths::AppPaths,
) {
    let remote_cleared = account_context
        .client
        .set_support_diagnostic_consent(false)
        .await
        .is_ok()
        && account_context
            .client
            .delete_all_support_diagnostics()
            .await
            .is_ok();
    if remote_cleared {
        return;
    }
    let owner_account_id = account_context.owner_account_id.as_str();
    if cue_core::update_settings(paths, |settings| {
        settings.support_diagnostics_upload_enabled = false;
        settings.support_diagnostics_upload_consent_granted = false;
        settings.support_diagnostics_upload_consent_account_id = None;
        settings.support_diagnostics_server_revocation_pending = true;
        settings.support_diagnostics_server_revocation_account_id =
            Some(owner_account_id.to_string());
    })
    .is_err()
    {
        tracing::error!(
            error_category = "support_consent_compensation_persist_failed",
            "support diagnostic consent compensation could not be persisted"
        );
    }
}

#[tauri::command]
pub fn get_data_controls(app: AppHandle) -> Result<DataControlsPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
        let settings = cue_core::load_settings(&paths).map_err(|e| e.to_string())?;
        Ok(data_controls_payload(
            &settings,
            &owner_guard.identity.owner,
        ))
    })
}

#[tauri::command]
pub fn set_cloud_sync_enabled(
    enabled: bool,
    app: AppHandle,
) -> Result<DataControlsPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let owner = &owner_guard.identity.owner;
    let owner_account_id = owner
        .db_owner_id()
        .ok_or_else(|| "Sign in before enabling saved-session sync.".to_string())?;
    owner_guard.with_current(&app, || {
        let account_context = DashboardAccountCloudContext::snapshot(&dashboard_trace_id())?;
        if account_context.owner_account_id != owner_account_id {
            return Err(account_changed_message());
        }
        account_context.ensure_current()?;
        let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
        let settings = cue_core::update_settings(&paths, |settings| {
            set_cloud_sync_for_account(settings, owner_account_id, enabled);
        })
        .map_err(|e| e.to_string())?;
        Ok(data_controls_payload(&settings, owner))
    })
}

#[tauri::command]
pub async fn set_support_diagnostics_upload_enabled(
    enabled: bool,
    app: AppHandle,
) -> Result<DataControlsPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let owner = &owner_guard.identity.owner;
    let owner_account_id = owner
        .db_owner_id()
        .ok_or_else(|| "Sign in before changing support diagnostics.".to_string())?;
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let account_context = DashboardAccountCloudContext::snapshot(&dashboard_trace_id())?;
    if account_context.owner_account_id != owner_account_id {
        return Err(account_changed_message());
    }
    account_context.ensure_current()?;
    if enabled {
        let existing = owner_guard.with_current(&app, || {
            cue_core::load_settings(&paths).map_err(|e| e.to_string())
        })?;
        let remote_consent_was_new =
            !existing.support_diagnostics_upload_allowed_for_account(Some(owner_account_id));
        if existing.support_diagnostics_server_revocation_pending {
            let detail = if existing
                .support_diagnostics_revocation_pending_for_account(Some(owner_account_id))
            {
                "Support diagnostic cleanup is still pending. Try again after cleanup finishes."
            } else {
                SUPPORT_CLEANUP_OWNER_MESSAGE
            };
            return Err(detail.to_string());
        }
        account_context
            .client
            .set_support_diagnostic_consent(true)
            .await
            .map_err(|_| "Bluey could not confirm support diagnostic consent yet.".to_string())?;
        if let Err(error) = account_context.ensure_current() {
            if remote_consent_was_new {
                compensate_support_diagnostic_grant(&account_context, &paths).await;
            }
            return Err(error);
        }
        let commit = owner_guard.with_current(&app, || {
            let settings = cue_core::update_settings(&paths, |settings| {
                settings.support_diagnostics_upload_consent_granted = true;
                settings.support_diagnostics_upload_enabled = true;
                settings.support_diagnostics_upload_consent_account_id =
                    Some(owner_account_id.to_string());
            })
            .map_err(|e| e.to_string())?;
            Ok(data_controls_payload(&settings, owner))
        });
        if commit.is_err() && remote_consent_was_new {
            compensate_support_diagnostic_grant(&account_context, &paths).await;
        }
        return commit;
    }

    // Stop uploads locally before any network work. If the server is offline,
    // the durable account-scoped outbox makes cleanup retry only for its owner.
    let existing = owner_guard.with_current(&app, || {
        cue_core::load_settings(&paths).map_err(|e| e.to_string())
    })?;
    if existing.support_diagnostics_revocation_waits_for_other_account(Some(owner_account_id)) {
        return Err(SUPPORT_CLEANUP_OWNER_MESSAGE.to_string());
    }
    let should_revoke = existing.support_diagnostics_consent_scoped_to(Some(owner_account_id))
        || existing.support_diagnostics_revocation_pending_for_account(Some(owner_account_id));
    if !should_revoke {
        return owner_guard.with_current(&app, || Ok(data_controls_payload(&existing, owner)));
    }
    let mut settings = owner_guard.with_current(&app, || {
        cue_core::update_settings(&paths, |settings| {
            let _ = begin_support_diagnostics_revocation(settings, owner_account_id);
        })
        .map_err(|e| e.to_string())
    })?;

    let server_cleanup_completed = account_context.ensure_current().is_ok()
        && account_context
            .client
            .set_support_diagnostic_consent(false)
            .await
            .is_ok()
        && account_context.ensure_current().is_ok()
        && account_context
            .client
            .delete_all_support_diagnostics()
            .await
            .is_ok()
        && account_context.ensure_current().is_ok();
    if server_cleanup_completed {
        settings = owner_guard.with_current(&app, || {
            cue_core::update_settings(&paths, |settings| {
                complete_support_diagnostics_revocation(settings, owner_account_id);
            })
            .map_err(|e| e.to_string())
        })?;
    } else {
        tracing::warn!(
            "support diagnostic upload stopped locally; server cleanup is queued for retry"
        );
    }
    owner_guard.with_current(&app, || Ok(data_controls_payload(&settings, owner)))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ContextWatchSettingsPayload {
    pub semantic_first: bool,
    pub screenshot_fallback: bool,
    pub interval_secs: u64,
    pub max_local_items: usize,
    pub excluded_apps: Vec<String>,
    pub excluded_domains: Vec<String>,
}

fn context_watch_settings_payload(
    settings: &cue_core::ContextWatchSettings,
) -> ContextWatchSettingsPayload {
    ContextWatchSettingsPayload {
        semantic_first: settings.semantic_first,
        screenshot_fallback: settings.screenshot_fallback,
        interval_secs: settings.interval_secs,
        max_local_items: settings.max_local_items,
        excluded_apps: settings.excluded_apps.clone(),
        excluded_domains: settings.excluded_domains.clone(),
    }
}

fn apply_context_watch_settings(
    current: &mut cue_core::CueSettings,
    requested: ContextWatchSettingsPayload,
) {
    // Semantic browser text remains the mandatory first boundary. The
    // dashboard may authorize screenshots only as an explicit fallback.
    current.context_watch.semantic_first = true;
    current.context_watch.screenshot_fallback = requested.screenshot_fallback;
    current.context_watch.interval_secs = requested.interval_secs;
    current.context_watch.max_local_items = requested.max_local_items;
    current.context_watch.excluded_apps = requested.excluded_apps;
    current.context_watch.excluded_domains = requested.excluded_domains;
}

#[tauri::command]
pub fn get_context_watch_settings() -> Result<ContextWatchSettingsPayload, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let settings = cue_core::load_settings(&paths).map_err(|e| e.to_string())?;
    Ok(context_watch_settings_payload(&settings.context_watch))
}

#[tauri::command]
pub fn update_context_watch_settings(
    settings: ContextWatchSettingsPayload,
) -> Result<ContextWatchSettingsPayload, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let saved = cue_core::update_settings(&paths, move |current| {
        apply_context_watch_settings(current, settings);
    })
    .map_err(|e| e.to_string())?;
    Ok(context_watch_settings_payload(&saved.context_watch))
}

#[tauri::command]
pub fn get_meeting_detection_ignored_apps() -> Result<Vec<String>, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let settings = cue_core::load_settings(&paths).map_err(|e| e.to_string())?;
    Ok(settings.meeting_detection_ignored_apps)
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MeetingDetectionSettingsPayload {
    pub enabled: bool,
    pub detection_refreshed: bool,
}

async fn reload_meeting_detection_settings() -> bool {
    match daemon_ipc(DaemonRequest::MeetingDetectionSettingsReload).await {
        Ok(DaemonResponse::Ok | DaemonResponse::Text { .. }) => true,
        Ok(DaemonResponse::Error { message }) => {
            tracing::warn!(%message, "meeting detection settings reload was rejected");
            false
        }
        Ok(_) => {
            tracing::warn!("meeting detection settings reload returned an unexpected response");
            false
        }
        Err(error) => {
            tracing::warn!(%error, "meeting detection settings reload failed");
            false
        }
    }
}

#[tauri::command]
pub fn get_meeting_detection_settings() -> Result<MeetingDetectionSettingsPayload, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let settings = cue_core::load_settings(&paths).map_err(|e| e.to_string())?;
    Ok(MeetingDetectionSettingsPayload {
        enabled: settings.meeting_detection_enabled,
        detection_refreshed: true,
    })
}

#[tauri::command]
pub async fn set_meeting_detection_enabled(
    enabled: bool,
) -> Result<MeetingDetectionSettingsPayload, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let settings = cue_core::update_settings(&paths, |settings| {
        settings.meeting_detection_enabled = enabled;
    })
    .map_err(|e| e.to_string())?;
    let detection_refreshed = reload_meeting_detection_settings().await;
    Ok(MeetingDetectionSettingsPayload {
        enabled: settings.meeting_detection_enabled,
        detection_refreshed,
    })
}

#[derive(Clone, Serialize)]
pub struct ClearMeetingDetectionIgnoredAppsPayload {
    pub apps: Vec<String>,
    pub detection_refreshed: bool,
}

#[tauri::command]
pub async fn clear_meeting_detection_ignored_apps(
) -> Result<ClearMeetingDetectionIgnoredAppsPayload, String> {
    let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
    let settings = cue_core::update_settings(&paths, |settings| {
        settings.meeting_detection_ignored_apps.clear();
    })
    .map_err(|e| e.to_string())?;

    let detection_refreshed = reload_meeting_detection_settings().await;

    Ok(ClearMeetingDetectionIgnoredAppsPayload {
        apps: settings.meeting_detection_ignored_apps,
        detection_refreshed,
    })
}

#[tauri::command]
pub fn save_stt_api_key(provider: String, key: String) -> Result<(), String> {
    cue_daemon::secrets::store_api_key(&provider, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_stt_api_key(provider: String) -> Result<Option<String>, String> {
    let raw = cue_daemon::secrets::load_api_key(&provider).map_err(|e| e.to_string())?;
    Ok(raw.map(|s| {
        // Mask all but the LAST 4 chars (codepoints, not bytes — string-slicing
        // by bytes panics on multi-byte UTF-8). Provider keys are normally ASCII
        // but this guards against future non-ASCII secrets.
        let total = s.chars().count();
        if total <= 4 {
            "****".to_string()
        } else {
            let suffix: String = s.chars().skip(total - 4).collect();
            format!("****{suffix}")
        }
    }))
}

#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<String>, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let mut names = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                names.push(name);
            }
        }
    }
    Ok(names)
}

#[tauri::command]
pub fn save_settings(
    settings: std::collections::HashMap<String, String>,
    db: State<DbState>,
) -> Result<(), String> {
    // Reject any keys that look like secrets BEFORE we open a db transaction.
    // Secrets must go through save_stt_api_key (keyring-backed). Returning an
    // explicit error makes accidental writes visible in dev tooling instead of
    // being silently dropped.
    for k in settings.keys() {
        if k.contains("api_key") {
            return Err(format!(
                "refusing to persist secret-shaped key {k} through save_settings; \
                 use save_stt_api_key instead"
            ));
        }
    }
    let db = db.0.lock().map_err(|e| e.to_string())?;
    for (k, v) in &settings {
        db.save_setting(k, v).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn load_settings(
    db: State<DbState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.load_all_settings().map_err(|e| e.to_string())
}

// ===== Phase 3 Round 5: Search, Export, Speakers =====

#[tauri::command]
pub fn search_transcripts(
    query: String,
    limit: Option<usize>,
    db: State<DbState>,
    app: AppHandle,
) -> Result<Vec<cue_daemon::db::search::TranscriptHit>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.search_transcripts_for_owner(
            owner_guard.identity.owner.db_owner_id(),
            &query,
            limit.unwrap_or(50),
        )
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn export_session_to_clipboard(
    id: String,
    format: String,
    app: AppHandle,
) -> Result<String, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let db_state: State<DbState> = app.state();
    owner_guard.with_current(&app, || {
        let db = db_state.0.lock().map_err(|e| e.to_string())?;
        match format.as_str() {
            "markdown" => db
                .export_session_markdown_for_owner(
                    owner_guard.identity.owner.db_owner_id(),
                    &id,
                    &cue_daemon::export::ExportOptions::default(),
                )
                .map_err(|e| e.to_string()),
            "text" => db
                .export_session_text_for_owner(owner_guard.identity.owner.db_owner_id(), &id)
                .map_err(|e| e.to_string()),
            "json" => db
                .export_session_json_for_owner(owner_guard.identity.owner.db_owner_id(), &id)
                .map_err(|e| e.to_string()),
            _ => Err(format!("unsupported format: {format}")),
        }
    })
}

#[tauri::command]
pub fn export_session_to_file(
    id: String,
    format: String,
    path: String,
    app: AppHandle,
) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let db_state: State<DbState> = app.state();
    owner_guard.with_current(&app, || {
        let db = db_state.0.lock().map_err(|e| e.to_string())?;
        let content = match format.as_str() {
            "markdown" => db
                .export_session_markdown_for_owner(
                    owner_guard.identity.owner.db_owner_id(),
                    &id,
                    &cue_daemon::export::ExportOptions::default(),
                )
                .map_err(|e| e.to_string())?,
            "text" => db
                .export_session_text_for_owner(owner_guard.identity.owner.db_owner_id(), &id)
                .map_err(|e| e.to_string())?,
            "json" => db
                .export_session_json_for_owner(owner_guard.identity.owner.db_owner_id(), &id)
                .map_err(|e| e.to_string())?,
            _ => return Err(format!("unsupported format: {format}")),
        };
        std::fs::write(&path, content).map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_speaker_name(
    session_id: String,
    speaker_id: i32,
    name: String,
    color: Option<String>,
    db: State<DbState>,
    app: AppHandle,
) -> Result<(), String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.set_speaker_name_for_owner(
            owner_guard.identity.owner.db_owner_id(),
            &session_id,
            speaker_id,
            &name,
            color.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn list_speakers(
    session_id: String,
    db: State<DbState>,
    app: AppHandle,
) -> Result<Vec<cue_daemon::db::speakers::SpeakerMapping>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.list_speakers_for_owner(owner_guard.identity.owner.db_owner_id(), &session_id)
            .map_err(|e| e.to_string())
    })
}

// ===== Phase 3 Round 6: Hotkey → daemon action commands =====

type DaemonResponseFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DaemonResponse, String>> + Send + 'a>>;

trait DaemonRequester {
    fn request(&mut self, request: DaemonRequest) -> DaemonResponseFuture<'_>;
}

struct AuthenticatedDaemonRequester<'a> {
    trace_id: &'a str,
}

impl DaemonRequester for AuthenticatedDaemonRequester<'_> {
    fn request(&mut self, request: DaemonRequest) -> DaemonResponseFuture<'_> {
        Box::pin(daemon_ipc_with_trace(request, self.trace_id))
    }
}

fn audio_pipeline_is_active(status: &AudioPipelineStatus) -> bool {
    status.capture.is_active()
        || (status.session_id.is_some()
            && !matches!(
                status.capture.state,
                AudioCaptureState::Stopped | AudioCaptureState::Failed
            ))
}

fn audio_pipeline_has_dual_sources(status: &AudioPipelineStatus) -> bool {
    status.config.system.enabled
        && status.config.microphone.enabled
        && status.capture.system.state != AudioSourceState::Failed
        && status.capture.microphone.state != AudioSourceState::Failed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublicAudioErrorKind {
    SignIn,
    Permission,
    Source,
    Timeout,
    Unavailable,
    Generic,
}

fn classify_audio_error(message: &str) -> PublicAudioErrorKind {
    let message = message.to_ascii_lowercase();
    if ["permission", "screen recording", "tcc", "access denied"]
        .iter()
        .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Permission
    } else if [
        "sign in",
        "signin",
        "not signed",
        "unauthorized",
        "login required",
        "account required",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::SignIn
    } else if ["timed out", "timeout", "deadline"]
        .iter()
        .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Timeout
    } else if [
        "audio source",
        "microphone",
        "system audio",
        "audio device",
        "audio backend",
        "source unavailable",
        "source failed",
        "coreaudio",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Source
    } else if [
        "daemon",
        "ipc",
        "connection",
        "transport",
        "closed",
        "offline",
        "refused",
        "broken pipe",
        "not running",
        "unexpected eof",
        "authentication failed",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        PublicAudioErrorKind::Unavailable
    } else {
        PublicAudioErrorKind::Generic
    }
}

fn public_audio_error(message: &str) -> String {
    match classify_audio_error(message) {
        PublicAudioErrorKind::SignIn => "Sign in to Bluey to start listening.",
        PublicAudioErrorKind::Permission => {
            "Allow microphone and system audio access, then try again."
        }
        PublicAudioErrorKind::Source => {
            "A required audio source is unavailable. Check audio settings and try again."
        }
        PublicAudioErrorKind::Timeout => "Bluey took too long to respond. Try again.",
        PublicAudioErrorKind::Unavailable => {
            "Bluey's local audio service is unavailable. Try again."
        }
        PublicAudioErrorKind::Generic => "Bluey couldn't update listening. Try again.",
    }
    .to_string()
}

fn public_audio_failure(operation: &str, message: &str) -> String {
    tracing::warn!(
        operation,
        kind = ?classify_audio_error(message),
        error_length = message.len(),
        "audio operation failed"
    );
    public_audio_error(message)
}

fn sanitize_audio_pipeline_status(mut status: AudioPipelineStatus) -> AudioPipelineStatus {
    let sanitize = |value: &mut Option<String>| {
        if let Some(message) = value.take() {
            *value = Some(public_audio_error(&message));
        }
    };

    sanitize(&mut status.capture.last_error);
    sanitize(&mut status.capture.system.last_error);
    sanitize(&mut status.capture.microphone.last_error);
    if status.capture.state == AudioCaptureState::Failed {
        sanitize(&mut status.note);
    }
    status
}

fn audio_status_from_response(
    response: DaemonResponse,
    operation: &str,
) -> Result<AudioPipelineStatus, String> {
    match response {
        DaemonResponse::AudioStatus { status } => Ok(sanitize_audio_pipeline_status(status)),
        DaemonResponse::Error { message } => Err(public_audio_failure(operation, &message)),
        _ => Err(public_audio_failure(operation, "unexpected response")),
    }
}

async fn daemon_listening_status_with<R: DaemonRequester + ?Sized>(
    requester: &mut R,
) -> Result<AudioPipelineStatus, String> {
    let response = requester
        .request(DaemonRequest::AudioStatus)
        .await
        .map_err(|message| public_audio_failure("query audio status", &message))?;
    audio_status_from_response(response, "query audio status")
}

async fn daemon_toggle_listening_with<R: DaemonRequester + ?Sized>(
    requester: &mut R,
    mic_device_id: Option<String>,
    fence: DaemonMutationFence,
) -> Result<AudioPipelineStatus, String> {
    let current = daemon_listening_status_with(requester).await?;
    let was_active = audio_pipeline_is_active(&current);
    let mut current_fence = fence.clone();
    current_fence.audio_session_id = current.session_id.clone();
    let (request, operation) = if was_active {
        (
            DaemonRequest::AudioStopBound {
                fence: current_fence,
            },
            "stop audio capture",
        )
    } else {
        (
            DaemonRequest::AudioStartBound {
                enable_system: true,
                enable_microphone: true,
                mic_device_id,
                fence: current_fence,
            },
            "start audio capture",
        )
    };

    let response = requester
        .request(request)
        .await
        .map_err(|message| public_audio_failure(operation, &message))?;
    let status = audio_status_from_response(response, operation)?;

    if !was_active && audio_pipeline_is_active(&status) && !audio_pipeline_has_dual_sources(&status)
    {
        let mut cleanup_fence = fence;
        cleanup_fence.audio_session_id = status.session_id.clone();
        if let Err(message) = requester
            .request(DaemonRequest::AudioStopBound {
                fence: cleanup_fence,
            })
            .await
        {
            tracing::warn!(
                kind = ?classify_audio_error(&message),
                "failed to stop partial audio capture"
            );
        }
        return Err(
            "Bluey couldn't start both audio sources, so listening was stopped. Check audio settings and try again."
                .to_string(),
        );
    }

    Ok(status)
}

const END_SESSION_SETTLE_DELAY: Duration = Duration::from_secs(3);

fn public_end_session_failure(message: &str) -> String {
    tracing::warn!(
        kind = ?classify_audio_error(message),
        error_length = message.len(),
        "end session failed"
    );
    match classify_audio_error(message) {
        PublicAudioErrorKind::SignIn => "Sign in to Bluey to end this session.",
        PublicAudioErrorKind::Timeout => "Bluey took too long to end the session. Try again.",
        PublicAudioErrorKind::Unavailable => {
            "Bluey's local service is unavailable, so the session could not be ended."
        }
        _ => "Bluey couldn't end the session. Try again.",
    }
    .to_string()
}

async fn daemon_end_session_with<R: DaemonRequester + ?Sized>(
    requester: &mut R,
    settle_delay: Duration,
    fence: DaemonMutationFence,
) -> Result<AudioPipelineStatus, String> {
    let current = daemon_listening_status_with(requester).await?;
    let should_stop = audio_pipeline_is_active(&current);
    let should_settle = should_stop
        || matches!(
            current.capture.state,
            AudioCaptureState::Stopping | AudioCaptureState::Stopped
        );
    let stopped = if should_stop {
        let mut stop_fence = fence.clone();
        stop_fence.audio_session_id = current.session_id.clone();
        let response = requester
            .request(DaemonRequest::AudioStopBound { fence: stop_fence })
            .await
            .map_err(|message| public_end_session_failure(&message))?;
        match response {
            DaemonResponse::AudioStatus { status } => Ok(sanitize_audio_pipeline_status(status)),
            DaemonResponse::Error { message } => Err(public_end_session_failure(&message)),
            _ => Err(public_end_session_failure("unexpected audio stop response")),
        }?
    } else {
        current
    };

    if should_settle && !settle_delay.is_zero() {
        tokio::time::sleep(settle_delay).await;
    }

    let response = requester
        .request(DaemonRequest::MeetingEndBound { fence })
        .await
        .map_err(|message| public_end_session_failure(&message))?;
    match response {
        DaemonResponse::Ok | DaemonResponse::Text { .. } | DaemonResponse::Recap { .. } => {
            Ok(stopped)
        }
        DaemonResponse::Error { message } => Err(public_end_session_failure(&message)),
        _ => Err(public_end_session_failure("unexpected response")),
    }
}

/// Return the daemon's current audio pipeline status.
#[tauri::command]
pub async fn daemon_listening_status(app: AppHandle) -> Result<AudioPipelineStatus, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let mut requester = AuthenticatedDaemonRequester {
        trace_id: &trace_id,
    };
    let status = daemon_listening_status_with(&mut requester).await?;
    owner_guard.with_current(&app, || Ok(status))
}

/// Toggle dual-source audio capture without ending the active meeting.
#[tauri::command]
pub async fn daemon_toggle_listening(
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<AudioPipelineStatus, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let trace_id = dashboard_trace_id();
    let mic_device_id = load_mic_device_from_settings(&db);
    let mut requester = AuthenticatedDaemonRequester {
        trace_id: &trace_id,
    };
    let result = daemon_toggle_listening_with(&mut requester, mic_device_id, fence).await;

    match result {
        Ok(status) => owner_guard.with_current(&app, || {
            let _ = app.emit("audio_pipeline_status", &status);
            Ok(status)
        }),
        Err(error) => owner_guard.with_current(&app, || {
            let _ = app.emit("audio_pipeline_error", &error);
            Err(error)
        }),
    }
}

/// Stop live audio, allow final STT tail frames to settle, and archive the
/// daemon's active meeting through the authenticated local IPC transport.
#[tauri::command]
pub async fn daemon_end_session(
    db: State<'_, DbState>,
    active: State<'_, ActiveSessionState>,
    app: AppHandle,
) -> Result<AudioPipelineStatus, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let mut requester = AuthenticatedDaemonRequester {
        trace_id: &trace_id,
    };
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let status = daemon_end_session_with(&mut requester, END_SESSION_SETTLE_DELAY, fence).await?;

    owner_guard.with_current(&app, || {
        match db.0.lock() {
            Ok(db) => {
                if let Err(error) =
                    db.save_active_session_for_owner(owner_guard.identity.owner.db_owner_id(), None)
                {
                    tracing::warn!(%error, "failed to clear active session after ending meeting");
                }
            }
            Err(error) => tracing::warn!(%error, "db lock poisoned after ending meeting"),
        }
        cache_active_session(&active, &owner_guard.identity.owner, None)?;

        let _ = app.emit("audio_pipeline_status", &status);
        let _ = app.emit("session:switched", SessionSwitchedPayload { id: None });
        let _ = app.emit("live_session_ended", ());
        Ok(status)
    })
}

/// Push-to-talk toggle. Since global shortcuts don't distinguish press/release,
/// this toggles audio capture on/off. When PTT is "enabled" conceptually, audio
/// only streams while toggled on. Each press cycles the state.
#[tauri::command]
pub async fn daemon_set_push_to_talk(
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<String, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    // Toggle audio: if audio is active, stop it; otherwise start mic-only.
    let status = daemon_ipc_with_trace(DaemonRequest::AudioStatus, &trace_id).await?;
    let is_active = match &status {
        DaemonResponse::AudioStatus { status } => {
            status.session_id.is_some()
                && !matches!(
                    status.capture.state,
                    cue_core::AudioCaptureState::Stopped | cue_core::AudioCaptureState::Failed
                )
        }
        _ => false,
    };
    let audio_session_id = match &status {
        DaemonResponse::AudioStatus { status } => status.session_id.clone(),
        _ => None,
    };
    let fence = dashboard_mutation_fence(&app, &owner_guard, audio_session_id)?;
    let resp = if is_active {
        daemon_ipc_with_trace(DaemonRequest::AudioStopBound { fence }, &trace_id).await?
    } else {
        let mic_device_id = load_mic_device_from_settings(&db);
        daemon_ipc_with_trace(
            DaemonRequest::AudioStartBound {
                enable_system: false,
                enable_microphone: true,
                mic_device_id,
                fence,
            },
            &trace_id,
        )
        .await?
    };
    owner_guard.ensure_current(&app)?;
    match resp {
        DaemonResponse::AudioStatus { status } => Ok(format!("audio: {:?}", status.capture.state)),
        DaemonResponse::Error { message } => Err(message),
        _ => Ok("ok".to_string()),
    }
}

/// Toggle overlay visibility via daemon IPC.
#[tauri::command]
pub async fn daemon_toggle_overlay() -> Result<String, String> {
    let resp = daemon_ipc(DaemonRequest::OverlayToggle).await?;
    match resp {
        DaemonResponse::Ok => Ok("ok".to_string()),
        DaemonResponse::Error { message } => Err(message),
        _ => Ok("ok".to_string()),
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ContextModeStatusPayload {
    pub active: bool,
    pub interval_secs: Option<u64>,
    pub context_items: usize,
    pub capture_generation: u64,
}

fn context_mode_status_from_response(
    response: DaemonResponse,
) -> Result<ContextModeStatusPayload, String> {
    match response {
        DaemonResponse::Status { state } => Ok(ContextModeStatusPayload {
            active: state.screen_capture_active,
            interval_secs: state.screen_capture_interval_secs,
            context_items: state.context_items,
            capture_generation: state.screen_capture_generation,
        }),
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

async fn current_context_mode_status() -> Result<ContextModeStatusPayload, String> {
    context_mode_status_from_response(daemon_ipc(DaemonRequest::Status).await?)
}

#[tauri::command]
pub async fn daemon_context_status(app: AppHandle) -> Result<ContextModeStatusPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let status = current_context_mode_status().await?;
    owner_guard.with_current(&app, || Ok(status))
}

#[tauri::command]
pub async fn daemon_context_start(
    interval_secs: u64,
    app: AppHandle,
) -> Result<ContextModeStatusPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let current = current_context_mode_status().await?;
    owner_guard.ensure_current(&app)?;
    let mut fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    fence.capture_generation = Some(current.capture_generation);
    let interval_secs = interval_secs.clamp(3, 300);
    match daemon_ipc(DaemonRequest::ScreenCaptureStartBound {
        interval_secs: Some(interval_secs),
        fence,
    })
    .await?
    {
        DaemonResponse::Text { .. } | DaemonResponse::Ok => {
            owner_guard.ensure_current(&app)?;
            let status = current_context_mode_status().await?;
            owner_guard.with_current(&app, || Ok(status))
        }
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
pub async fn daemon_context_stop(app: AppHandle) -> Result<ContextModeStatusPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let current = current_context_mode_status().await?;
    owner_guard.ensure_current(&app)?;
    let mut fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    fence.capture_generation = Some(current.capture_generation);
    match daemon_ipc(DaemonRequest::ScreenCaptureStopBound { fence }).await? {
        DaemonResponse::Text { .. } | DaemonResponse::Ok => {
            owner_guard.ensure_current(&app)?;
            let status = current_context_mode_status().await?;
            owner_guard.with_current(&app, || Ok(status))
        }
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[tauri::command]
pub async fn daemon_capture_active_page(
    app: AppHandle,
) -> Result<ContextModeStatusPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    match daemon_ipc(DaemonRequest::ActivePageCaptureBound { fence }).await? {
        DaemonResponse::ContextItems { .. } => {
            owner_guard.ensure_current(&app)?;
            let status = current_context_mode_status().await?;
            owner_guard.with_current(&app, || Ok(status))
        }
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("unexpected daemon response: {other:?}")),
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ContextItemSummaryPayload {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub processing_status: String,
    pub created_at: String,
    pub context_mode_observation: bool,
    pub answer_context_role: cue_core::AnswerContextRole,
}

fn context_item_summary_payload(item: cue_core::ContextArtifact) -> ContextItemSummaryPayload {
    ContextItemSummaryPayload {
        id: item.id.to_string(),
        title: item.title,
        kind: item.kind.to_string(),
        processing_status: item.processing_status.to_string(),
        created_at: item.created_at,
        answer_context_role: item.answer_context_role,
        context_mode_observation: item
            .note
            .as_deref()
            .is_some_and(|note| note.contains("Context mode observation.")),
    }
}

fn validate_context_role_confirmation(
    answer_context_role: cue_core::AnswerContextRole,
    confirmed_by_user: bool,
) -> Result<(), String> {
    if answer_context_role == cue_core::AnswerContextRole::UserConfirmedStory && !confirmed_by_user
    {
        return Err(
            "My confirmed story requires your explicit confirmation that this item describes your own lived experience."
                .to_string(),
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn daemon_set_context_role(
    artifact_id: String,
    answer_context_role: cue_core::AnswerContextRole,
    confirmed_by_user: bool,
    app: AppHandle,
) -> Result<ContextItemSummaryPayload, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    validate_context_role_confirmation(answer_context_role, confirmed_by_user)?;
    let id = Uuid::parse_str(artifact_id.trim())
        .map_err(|error| format!("invalid context artifact ID: {error}"))?;
    let fence = dashboard_mutation_fence(&app, &owner_guard, None)?;
    let response = daemon_ipc(DaemonRequest::ContextRoleSetBound {
        id,
        answer_context_role,
        fence,
    })
    .await?;
    owner_guard.with_current(&app, || match response {
        DaemonResponse::ContextItems { items } => items
            .into_iter()
            .find(|item| item.id == id)
            .map(context_item_summary_payload)
            .ok_or_else(|| "daemon did not return the updated context item".to_string()),
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("unexpected daemon response: {other:?}")),
    })
}

#[tauri::command]
pub async fn daemon_context_items(
    app: AppHandle,
) -> Result<Vec<ContextItemSummaryPayload>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let response = daemon_ipc(DaemonRequest::ContextList).await?;
    owner_guard.with_current(&app, || match response {
        DaemonResponse::ContextItems { items } => Ok(items
            .into_iter()
            .map(context_item_summary_payload)
            .collect()),
        DaemonResponse::Error { message } => Err(message),
        other => Err(format!("unexpected daemon response: {other:?}")),
    })
}

/// Read the `audio.mic_device` setting from the dashboard DB.
fn load_mic_device_from_settings(db: &State<DbState>) -> Option<String> {
    let db = db.0.lock().ok()?;
    db.load_setting("audio.mic_device")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

// ===== Tests =====

// ===== Phase 3 Round 6: Permission UX =====

/// Open the OS privacy settings pane for the given audio source.
/// On macOS, opens System Preferences to the relevant Privacy pane.
/// On Windows, opens the Settings app to the microphone privacy page.
#[tauri::command]
pub fn open_privacy_settings(source: String) -> Result<(), String> {
    open_privacy_settings_inner(&source)
}

/// Returns the (program, args) tuple for opening privacy settings on the current platform.
/// Exposed for testing without actually spawning a process.
pub fn privacy_settings_command(source: &str) -> Result<(&'static str, Vec<String>), String> {
    #[cfg(target_os = "macos")]
    {
        let section = match source {
            "microphone" => "Privacy_Microphone",
            _ => "Privacy_ScreenCapture",
        };
        Ok((
            "open",
            vec![format!(
                "x-apple.systempreferences:com.apple.preference.security?{section}"
            )],
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let section = match source {
            "microphone" => "privacy-microphone",
            _ => "privacy-microphone",
        };
        Ok((
            "cmd",
            vec![
                "/C".to_string(),
                "start".to_string(),
                format!("ms-settings:{section}"),
            ],
        ))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = source;
        Err("open_privacy_settings: not supported on this platform".into())
    }
}

/// Platform-specific privacy settings launcher.
fn open_privacy_settings_inner(source: &str) -> Result<(), String> {
    let (program, args) = privacy_settings_command(source)?;
    std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Payload for the audio_permission_denied event.
#[derive(Clone, Serialize)]
pub struct PermissionDeniedPayload {
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioPermissionVerification {
    Allowed,
    Denied,
    NeedsListening,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioPermissionSourcePayload {
    pub source: String,
    pub verification: AudioPermissionVerification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AudioPermissionPollPayload {
    pub sources: Vec<AudioPermissionSourcePayload>,
}

fn audio_permission_verification(
    status: &AudioPipelineStatus,
    source: AudioSourceKind,
) -> AudioPermissionVerification {
    let source_status = match source {
        AudioSourceKind::System => &status.capture.system,
        AudioSourceKind::Microphone => &status.capture.microphone,
    };
    let source_reports_permission_denial = source_status
        .last_error
        .as_deref()
        .is_some_and(|message| classify_audio_error(message) == PublicAudioErrorKind::Permission);

    if status.capture.permission_denied_source == Some(source) || source_reports_permission_denial {
        AudioPermissionVerification::Denied
    } else if source_status.chunks_captured > 0 && source_status.last_sequence.is_some() {
        // A successfully received source chunk is the positive verification
        // boundary. Merely opening Settings or observing an idle/planned
        // pipeline does not prove that the OS granted capture access.
        AudioPermissionVerification::Allowed
    } else {
        AudioPermissionVerification::NeedsListening
    }
}

fn audio_permission_poll_payload(status: &AudioPipelineStatus) -> AudioPermissionPollPayload {
    AudioPermissionPollPayload {
        sources: [AudioSourceKind::Microphone, AudioSourceKind::System]
            .into_iter()
            .map(|source| AudioPermissionSourcePayload {
                source: source.default_label().to_string(),
                verification: audio_permission_verification(status, source),
            })
            .collect(),
    }
}

/// Emit a permission-denied event to the dashboard for testing/integration.
/// In production, the daemon capture code calls this when it detects denial.
#[tauri::command]
pub fn emit_permission_denied(source: String, app: AppHandle) -> Result<(), String> {
    app.emit(
        "audio_permission_denied",
        PermissionDeniedPayload { source },
    )
    .map_err(|e| e.to_string())
}

/// Poll daemon audio status and return source-specific verification.
///
/// A warning is cleared only after the daemon has successfully received a
/// chunk from that source. An idle/planned pipeline remains `needs_listening`
/// because it cannot prove that the OS permission is now allowed.
#[tauri::command]
pub async fn poll_audio_permission(app: AppHandle) -> Result<AudioPermissionPollPayload, String> {
    let status = match daemon_ipc(DaemonRequest::AudioStatus).await? {
        DaemonResponse::AudioStatus { status } => status,
        DaemonResponse::Error { message } => {
            return Err(public_audio_failure("verify audio permission", &message));
        }
        _ => return Err("Bluey could not read the current audio permission status.".to_string()),
    };
    let payload = audio_permission_poll_payload(&status);
    for source in &payload.sources {
        let event = match source.verification {
            AudioPermissionVerification::Allowed => "audio_permission_allowed",
            AudioPermissionVerification::Denied => "audio_permission_denied",
            AudioPermissionVerification::NeedsListening => continue,
        };
        app.emit(
            event,
            PermissionDeniedPayload {
                source: source.source.clone(),
            },
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(payload)
}

// ===== Phase 3 Round 9: LLM / Cue commands =====

#[tauri::command]
pub fn save_llm_api_key(provider: String, key: String) -> Result<(), String> {
    cue_daemon::secrets::store_api_key(&format!("llm_{provider}"), &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_llm_providers() -> Result<Vec<String>, String> {
    Ok(vec![
        "anthropic".to_string(),
        "openai".to_string(),
        "ollama".to_string(),
    ])
}

#[tauri::command]
pub fn list_responses(
    session_id: String,
    limit: usize,
    db: State<DbState>,
    app: AppHandle,
) -> Result<Vec<cue_daemon::llm::CueResponse>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let db = db.0.lock().map_err(|e| e.to_string())?;
        db.list_cue_responses_for_dashboard_owner(
            owner_guard.identity.owner.db_owner_id(),
            &session_id,
            limit,
        )
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub fn set_llm_chain(providers: Vec<String>, db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    let chain = providers.join(",");
    db.save_setting("llm.chain", &chain)
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct FakeDaemon {
        responses: VecDeque<Result<DaemonResponse, String>>,
        requests: Vec<DaemonRequest>,
    }

    impl FakeDaemon {
        fn with_responses(responses: Vec<DaemonResponse>) -> Self {
            Self {
                responses: responses.into_iter().map(Ok).collect(),
                requests: Vec::new(),
            }
        }
    }

    impl DaemonRequester for FakeDaemon {
        fn request(&mut self, request: DaemonRequest) -> DaemonResponseFuture<'_> {
            self.requests.push(request);
            let response = self
                .responses
                .pop_front()
                .expect("fake daemon response for request");
            Box::pin(std::future::ready(response))
        }
    }

    fn active_audio_status() -> AudioPipelineStatus {
        AudioPipelineStatus::simulated("session-active", cue_core::AudioCaptureConfig::default())
    }

    fn test_mutation_fence() -> DaemonMutationFence {
        DaemonMutationFence {
            owner_account_id: None,
            credential_generation: None,
            meeting_id: None,
            audio_session_id: None,
            capture_generation: None,
        }
    }

    fn linked_account(account_id: Option<&str>, user_id: &str) -> cue_core::AccountConfig {
        let mut account = cue_core::AccountConfig::local();
        account.provider = "bluey".to_string();
        account.cloud_account_id = account_id.map(str::to_string);
        account.user_id = user_id.to_string();
        account
    }

    fn tokens(email: &str) -> cue_cloud_client::Tokens {
        cue_cloud_client::Tokens {
            access: "access-token".to_string(),
            refresh: "refresh-token".to_string(),
            email: email.to_string(),
        }
    }

    fn credentials(access: &str, email: &str) -> cue_cloud_client::Tokens {
        cue_cloud_client::Tokens {
            access: access.to_string(),
            refresh: format!("refresh-{access}"),
            email: email.to_string(),
        }
    }

    fn dashboard_test_client(
        store: Arc<cue_cloud_client::tokens::MemoryStore>,
    ) -> cue_cloud_client::CloudClient {
        cue_cloud_client::CloudClient::new(
            cue_cloud_client::client::ClientConfig {
                base_url: "https://bluey.test".to_string(),
                user_agent: "bluey-dashboard-test".to_string(),
                timeout: Duration::from_secs(1),
                trace_id: None,
            },
            store,
        )
        .unwrap()
    }

    #[test]
    fn dashboard_delayed_clear_preserves_a2_and_b_but_clears_exact_snapshot() {
        let store = Arc::new(cue_cloud_client::tokens::MemoryStore::new());
        cue_cloud_client::TokenStore::save(store.as_ref(), &credentials("a1", "a@example.com"))
            .unwrap();
        let stale_a1 = dashboard_test_client(store.clone());
        let captured_a1 = stale_a1.credential_snapshot().unwrap();

        let refreshed_a2 = credentials("a2", "a@example.com");
        cue_cloud_client::TokenStore::save(store.as_ref(), &refreshed_a2).unwrap();
        assert!(!clear_captured_dashboard_credentials(&stale_a1, &captured_a1).unwrap());
        assert_eq!(
            cue_cloud_client::TokenStore::load(store.as_ref()).unwrap(),
            Some(refreshed_a2)
        );

        let stale_a2 = dashboard_test_client(store.clone());
        let captured_a2 = stale_a2.credential_snapshot().unwrap();
        let replacement_b = credentials("b1", "b@example.com");
        cue_cloud_client::TokenStore::save(store.as_ref(), &replacement_b).unwrap();
        assert!(!clear_captured_dashboard_credentials(&stale_a2, &captured_a2).unwrap());
        assert_eq!(
            cue_cloud_client::TokenStore::load(store.as_ref()).unwrap(),
            Some(replacement_b)
        );

        let current_b = dashboard_test_client(store.clone());
        let captured_b = current_b.credential_snapshot().unwrap();
        assert!(clear_captured_dashboard_credentials(&current_b, &captured_b).unwrap());
        assert_eq!(
            cue_cloud_client::TokenStore::load(store.as_ref()).unwrap(),
            None
        );
    }

    fn owner_guard_for_test(owner: DashboardOwner, generation: u64) -> DashboardOwnerGuard {
        let user_id = match &owner {
            DashboardOwner::Local => "local-user".to_string(),
            DashboardOwner::SignedIn(owner) => format!("{owner}@example.test"),
        };
        let cloud_account_id = owner.db_owner_id().map(str::to_string);
        DashboardOwnerGuard {
            identity: DashboardOwnerIdentity {
                owner,
                credential_generation: generation,
            },
            account_file_stamp: DashboardAccountFileStamp {
                present: true,
                provider: if cloud_account_id.is_some() {
                    "bluey".to_string()
                } else {
                    "local".to_string()
                },
                cloud_account_id,
                user_id,
                credential_generation: generation,
                token_configured: generation > 0,
            },
            transition_generation: 7,
        }
    }

    #[test]
    fn dashboard_owner_prefers_cloud_id_and_requires_verified_fallback() {
        let cloud_account = linked_account(Some("account-123"), "person@example.com");
        assert_eq!(
            resolve_dashboard_owner(Some(&cloud_account), Some(&tokens("person@example.com")))
                .unwrap(),
            DashboardOwner::SignedIn("account-123".to_string())
        );

        let legacy_account = linked_account(None, "person@example.com");
        assert_eq!(
            resolve_dashboard_owner(Some(&legacy_account), Some(&tokens("person@example.com")))
                .unwrap(),
            DashboardOwner::SignedIn("person@example.com".to_string())
        );
        assert!(
            resolve_dashboard_owner(Some(&legacy_account), Some(&tokens("other@example.com")))
                .is_err()
        );
        assert_eq!(
            resolve_dashboard_owner(Some(&cloud_account), None).unwrap(),
            DashboardOwner::Local
        );
    }

    #[test]
    fn external_account_replacement_blocks_provider_dispatch_before_network_call() {
        let guard = owner_guard_for_test(DashboardOwner::SignedIn("account-a".to_string()), 11);
        let account_b_stamp = DashboardAccountFileStamp {
            present: true,
            provider: "bluey".to_string(),
            cloud_account_id: Some("account-b".to_string()),
            user_id: "b@example.test".to_string(),
            credential_generation: 12,
            token_configured: true,
        };
        let network_calls = std::sync::atomic::AtomicUsize::new(0);

        let result = guard
            .validate_account_file_stamp(&account_b_stamp)
            .map(|()| {
                network_calls.fetch_add(1, Ordering::Relaxed);
            });

        assert!(result.is_err());
        assert_eq!(network_calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn stream_chunk_guard_is_memory_only_and_cancels_on_external_writer_signal() {
        let guard = owner_guard_for_test(DashboardOwner::SignedIn("account-a".to_string()), 11);
        let cache = DashboardOwnerCache {
            owner: Some(DashboardOwner::SignedIn("account-a".to_string())),
            transitioning: false,
            transition_generation: 7,
        };
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let token = DashboardOwnerStreamToken {
            cancelled: cancel_rx,
        };

        for _ in 0..10_000 {
            token.validate_fast(&guard, &cache).unwrap();
        }
        cancel_tx.send(true).unwrap();
        assert!(token.validate_fast(&guard, &cache).is_err());
    }

    #[test]
    fn data_controls_are_scoped_to_the_current_account() {
        let owner_a = DashboardOwner::SignedIn("account-a".to_string());
        let owner_b = DashboardOwner::SignedIn("account-b".to_string());
        let mut settings = cue_core::CueSettings::default();
        set_cloud_sync_for_account(&mut settings, "account-a", true);
        settings.support_diagnostics_upload_enabled = true;
        settings.support_diagnostics_upload_consent_granted = true;
        settings.support_diagnostics_upload_consent_account_id = Some("account-a".to_string());

        let controls_a = data_controls_payload(&settings, &owner_a);
        assert!(controls_a.account_scope_available);
        assert!(controls_a.cloud_sync_enabled);
        assert!(controls_a.support_diagnostics_upload_enabled);

        let controls_b = data_controls_payload(&settings, &owner_b);
        assert!(controls_b.account_scope_available);
        assert!(!controls_b.cloud_sync_enabled);
        assert!(!controls_b.support_diagnostics_upload_enabled);

        let local = data_controls_payload(&settings, &DashboardOwner::Local);
        assert!(!local.account_scope_available);
        assert!(!local.cloud_sync_enabled);
        assert!(!local.support_diagnostics_upload_enabled);
    }

    #[test]
    fn support_cleanup_outbox_never_changes_owner() {
        let owner_a = DashboardOwner::SignedIn("account-a".to_string());
        let owner_b = DashboardOwner::SignedIn("account-b".to_string());
        let mut settings = cue_core::CueSettings {
            support_diagnostics_upload_enabled: true,
            support_diagnostics_upload_consent_granted: true,
            support_diagnostics_upload_consent_account_id: Some("account-a".to_string()),
            ..cue_core::CueSettings::default()
        };

        assert!(begin_support_diagnostics_revocation(&mut settings, "account-a").unwrap());
        assert_eq!(
            settings
                .support_diagnostics_server_revocation_account_id
                .as_deref(),
            Some("account-a")
        );
        let controls_b = data_controls_payload(&settings, &owner_b);
        assert!(!controls_b.support_diagnostics_server_cleanup_pending);
        assert!(controls_b.support_diagnostics_cleanup_waiting_for_another_account);
        assert!(begin_support_diagnostics_revocation(&mut settings, "account-b").is_err());

        complete_support_diagnostics_revocation(&mut settings, "account-b");
        assert!(settings.support_diagnostics_server_revocation_pending);
        complete_support_diagnostics_revocation(&mut settings, "account-a");
        assert!(!settings.support_diagnostics_server_revocation_pending);
        assert!(settings
            .support_diagnostics_server_revocation_account_id
            .is_none());

        let controls_a = data_controls_payload(&settings, &owner_a);
        assert!(!controls_a.support_diagnostics_server_cleanup_pending);
    }

    #[test]
    fn dashboard_session_helpers_isolate_local_and_signed_in_owners() {
        let db = cue_daemon::db::Database::open(":memory:").unwrap();
        let local_owner = DashboardOwner::Local;
        let owner_a = DashboardOwner::SignedIn("account-a".to_string());
        let owner_b = DashboardOwner::SignedIn("account-b".to_string());
        let local = db.create_session(Some("Local".to_string())).unwrap();
        let session_a = db
            .create_session_for_owner(owner_a.db_owner_id(), Some("Owner A".to_string()))
            .unwrap();
        let session_b = db
            .create_session_for_owner(owner_b.db_owner_id(), Some("Owner B".to_string()))
            .unwrap();

        assert_eq!(
            dashboard_list_sessions(&db, &local_owner).unwrap()[0].id,
            local.id
        );
        assert_eq!(
            dashboard_list_sessions(&db, &owner_a).unwrap()[0].id,
            session_a.id
        );
        assert_eq!(
            dashboard_list_sessions(&db, &owner_b).unwrap()[0].id,
            session_b.id
        );
        assert!(dashboard_get_session(&db, &owner_b, session_a.id)
            .unwrap()
            .is_none());

        db.update_session_title_for_owner(owner_a.db_owner_id(), session_a.id, "Renamed A")
            .unwrap();
        assert_eq!(
            dashboard_get_session(&db, &owner_a, session_a.id)
                .unwrap()
                .unwrap()
                .title,
            "Renamed A"
        );
        assert_eq!(
            dashboard_get_session(&db, &owner_b, session_b.id)
                .unwrap()
                .unwrap()
                .title,
            "Owner B"
        );
    }

    #[test]
    fn daemon_lifecycle_projection_preserves_exact_ids_and_owner_scope() {
        let db = cue_daemon::db::Database::open(":memory:").unwrap();
        let owner = DashboardOwner::SignedIn("account-a".to_string());
        let id = Uuid::new_v4();
        let lifecycle = DaemonSessionLifecycle {
            changed: Some(DaemonSessionRecord {
                id,
                owner_account_id: Some("account-a".to_string()),
                title: "Canonical session".to_string(),
                started_at: "1000".to_string(),
                ended_at: None,
                active: true,
            }),
            replaced: None,
            deleted: None,
            active_session_id: Some(id),
        };

        let projected = project_session_lifecycle(&db, &owner, &lifecycle)
            .unwrap()
            .expect("changed session");
        assert_eq!(projected.id, id);
        assert_eq!(projected.title, "Canonical session");
        assert_eq!(projected.status, SessionStatus::Active);
        assert_eq!(
            db.load_active_session_for_owner(owner.db_owner_id())
                .unwrap(),
            Some(id)
        );
        assert!(db.get_session(id).unwrap().is_none());
        assert!(db
            .get_session_for_owner(Some("account-b"), id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn daemon_lifecycle_projection_archives_replaced_session_on_switch() {
        let db = cue_daemon::db::Database::open(":memory:").unwrap();
        let owner = DashboardOwner::Local;
        let old_id = Uuid::new_v4();
        let new_id = Uuid::new_v4();
        db.ensure_session_record(old_id, "Old", 1000, 1000).unwrap();
        db.save_active_session(Some(old_id)).unwrap();
        let lifecycle = DaemonSessionLifecycle {
            changed: Some(DaemonSessionRecord {
                id: new_id,
                owner_account_id: None,
                title: "New".to_string(),
                started_at: "2000".to_string(),
                ended_at: None,
                active: true,
            }),
            replaced: Some(DaemonSessionRecord {
                id: old_id,
                owner_account_id: None,
                title: "Old".to_string(),
                started_at: "1000".to_string(),
                ended_at: Some("3000".to_string()),
                active: false,
            }),
            deleted: None,
            active_session_id: Some(new_id),
        };

        project_session_lifecycle(&db, &owner, &lifecycle).unwrap();
        assert_eq!(
            db.get_session(old_id).unwrap().unwrap().status,
            SessionStatus::Archived
        );
        assert_eq!(
            db.get_session(new_id).unwrap().unwrap().status,
            SessionStatus::Active
        );
        assert_eq!(db.load_active_session().unwrap(), Some(new_id));
    }

    #[test]
    fn daemon_lifecycle_projection_rejects_cross_owner_records() {
        let db = cue_daemon::db::Database::open(":memory:").unwrap();
        let owner = DashboardOwner::SignedIn("account-a".to_string());
        let id = Uuid::new_v4();
        let lifecycle = DaemonSessionLifecycle {
            changed: Some(DaemonSessionRecord {
                id,
                owner_account_id: Some("account-b".to_string()),
                title: "Foreign".to_string(),
                started_at: "1000".to_string(),
                ended_at: None,
                active: true,
            }),
            replaced: None,
            deleted: None,
            active_session_id: Some(id),
        };

        assert!(project_session_lifecycle(&db, &owner, &lifecycle).is_err());
        assert!(db
            .get_session_for_owner(Some("account-a"), id)
            .unwrap()
            .is_none());
        assert!(db
            .get_session_for_owner(Some("account-b"), id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn active_session_cache_and_persistence_are_owner_scoped() {
        let db = cue_daemon::db::Database::open(":memory:").unwrap();
        let owner_a = DashboardOwner::SignedIn("account-a".to_string());
        let owner_b = DashboardOwner::SignedIn("account-b".to_string());
        let session_a = db
            .create_session_for_owner(owner_a.db_owner_id(), Some("Owner A".to_string()))
            .unwrap();
        let session_b = db
            .create_session_for_owner(owner_b.db_owner_id(), Some("Owner B".to_string()))
            .unwrap();
        db.save_active_session_for_owner(owner_a.db_owner_id(), Some(session_a.id))
            .unwrap();
        db.save_active_session_for_owner(owner_b.db_owner_id(), Some(session_b.id))
            .unwrap();
        let active = ActiveSessionState(Mutex::new(None));

        assert_eq!(
            active_session_for_owner(&db, &active, &owner_a).unwrap(),
            Some(session_a.id)
        );
        assert_eq!(
            active_session_for_owner(&db, &active, &owner_b).unwrap(),
            Some(session_b.id)
        );
    }

    #[test]
    fn account_switch_clears_cached_visible_session_even_for_reauthentication() {
        let owner_a = DashboardOwner::SignedIn("account-a".to_string());
        let owner_b = DashboardOwner::SignedIn("account-b".to_string());
        let mut cached_owner = Some(owner_a.clone());
        let mut cached_active = Some(ActiveSessionSelection {
            owner: owner_a.clone(),
            id: Some(Uuid::new_v4()),
        });

        assert!(update_cached_owner(
            &mut cached_owner,
            &mut cached_active,
            Some(owner_b.clone()),
            false
        ));
        assert_eq!(cached_owner, Some(owner_b.clone()));
        assert!(cached_active.is_none());

        cached_active = Some(ActiveSessionSelection {
            owner: owner_b.clone(),
            id: Some(Uuid::new_v4()),
        });
        assert!(update_cached_owner(
            &mut cached_owner,
            &mut cached_active,
            Some(owner_b),
            true
        ));
        assert!(cached_active.is_none());
    }

    #[tokio::test]
    async fn daemon_ipc_wraps_dashboard_request_with_trace() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            writer.write_all(br#"{"type":"pong"}"#).await.unwrap();
            writer.write_all(b"\n").await.unwrap();
            line
        });

        let response =
            daemon_ipc_with_trace_to_addr(DaemonRequest::Ping, "dashboard-smoke-trace", &addr)
                .await
                .unwrap();
        assert!(matches!(response, DaemonResponse::Pong));

        let line = server.await.unwrap();
        let request: DaemonRequest = serde_json::from_str(line.trim()).unwrap();
        match request {
            DaemonRequest::WithTrace { trace_id, request } => {
                assert_eq!(trace_id, "dashboard-smoke-trace");
                assert!(matches!(*request, DaemonRequest::Ping));
            }
            other => panic!("dashboard request was not trace-wrapped: {other:?}"),
        }
    }

    #[tokio::test]
    async fn listening_toggle_starts_dual_source_audio_with_saved_mic() {
        let started = active_audio_status();
        let mut daemon = FakeDaemon::with_responses(vec![
            DaemonResponse::AudioStatus {
                status: AudioPipelineStatus::idle(),
            },
            DaemonResponse::AudioStatus {
                status: started.clone(),
            },
        ]);

        let result = daemon_toggle_listening_with(
            &mut daemon,
            Some("saved-mic-id".to_string()),
            test_mutation_fence(),
        )
        .await
        .unwrap();
        assert_eq!(result, started);
        assert_eq!(daemon.requests.len(), 2);
        assert!(matches!(&daemon.requests[0], DaemonRequest::AudioStatus));
        assert!(matches!(
            &daemon.requests[1],
            DaemonRequest::AudioStartBound {
                enable_system: true,
                enable_microphone: true,
                mic_device_id: Some(mic_device_id),
                ..
            } if mic_device_id == "saved-mic-id"
        ));
    }

    #[tokio::test]
    async fn listening_toggle_stops_starting_audio_without_a_session_id() {
        let mut starting = AudioPipelineStatus::idle();
        starting.capture.state = AudioCaptureState::Starting;
        let mut stopped = starting.clone();
        stopped.capture.state = AudioCaptureState::Stopped;
        let mut daemon = FakeDaemon::with_responses(vec![
            DaemonResponse::AudioStatus { status: starting },
            DaemonResponse::AudioStatus {
                status: stopped.clone(),
            },
        ]);

        let result = daemon_toggle_listening_with(&mut daemon, None, test_mutation_fence())
            .await
            .unwrap();
        assert_eq!(result, stopped);
        assert_eq!(daemon.requests.len(), 2);
        assert!(matches!(&daemon.requests[0], DaemonRequest::AudioStatus));
        assert!(matches!(
            &daemon.requests[1],
            DaemonRequest::AudioStopBound { .. }
        ));
    }

    #[tokio::test]
    async fn listening_toggle_stops_a_partial_dual_source_start() {
        let mut partial = active_audio_status();
        partial.config.system.enabled = false;
        let mut stopped = partial.clone();
        stopped.capture.state = AudioCaptureState::Stopped;
        let mut daemon = FakeDaemon::with_responses(vec![
            DaemonResponse::AudioStatus {
                status: AudioPipelineStatus::idle(),
            },
            DaemonResponse::AudioStatus { status: partial },
            DaemonResponse::AudioStatus { status: stopped },
        ]);

        let error = daemon_toggle_listening_with(
            &mut daemon,
            Some("saved-mic-id".to_string()),
            test_mutation_fence(),
        )
        .await
        .unwrap_err();
        assert!(error.contains("both audio sources"));
        assert!(!error.contains("saved-mic-id"));
        assert_eq!(daemon.requests.len(), 3);
        assert!(matches!(
            &daemon.requests[2],
            DaemonRequest::AudioStopBound { .. }
        ));
    }

    #[tokio::test]
    async fn listening_status_returns_a_public_error() {
        let mut daemon = FakeDaemon::with_responses(vec![DaemonResponse::Error {
            message: "audio backend unavailable at /private/tmp/device".to_string(),
        }]);

        let error = daemon_listening_status_with(&mut daemon).await.unwrap_err();
        assert_eq!(
            error,
            "A required audio source is unavailable. Check audio settings and try again."
        );
        assert!(!error.contains("/private/tmp"));
        assert!(matches!(&daemon.requests[0], DaemonRequest::AudioStatus));
    }

    #[tokio::test]
    async fn end_session_stops_audio_then_settles_before_meeting_end() {
        let active = active_audio_status();
        let stopped = active.clone().stopped();
        let mut daemon = FakeDaemon::with_responses(vec![
            DaemonResponse::AudioStatus { status: active },
            DaemonResponse::AudioStatus {
                status: stopped.clone(),
            },
            DaemonResponse::Text {
                text: "Meeting ended.".to_string(),
            },
        ]);

        let result = daemon_end_session_with(&mut daemon, Duration::ZERO, test_mutation_fence())
            .await
            .unwrap();
        assert_eq!(result, stopped);
        assert_eq!(daemon.requests.len(), 3);
        assert!(matches!(&daemon.requests[0], DaemonRequest::AudioStatus));
        assert!(matches!(
            &daemon.requests[1],
            DaemonRequest::AudioStopBound { .. }
        ));
        assert!(matches!(
            &daemon.requests[2],
            DaemonRequest::MeetingEndBound { .. }
        ));
    }

    #[tokio::test]
    async fn end_session_still_ends_an_idle_meeting_without_restarting_audio() {
        let idle = AudioPipelineStatus::idle();
        let mut daemon = FakeDaemon::with_responses(vec![
            DaemonResponse::AudioStatus {
                status: idle.clone(),
            },
            DaemonResponse::Text {
                text: "Meeting ended.".to_string(),
            },
        ]);

        assert_eq!(
            daemon_end_session_with(&mut daemon, Duration::ZERO, test_mutation_fence())
                .await
                .unwrap(),
            idle
        );
        assert_eq!(daemon.requests.len(), 2);
        assert!(matches!(&daemon.requests[0], DaemonRequest::AudioStatus));
        assert!(matches!(
            &daemon.requests[1],
            DaemonRequest::MeetingEndBound { .. }
        ));
    }

    #[tokio::test]
    async fn end_session_surfaces_daemon_failure_without_reporting_success() {
        let mut daemon = FakeDaemon::with_responses(vec![
            DaemonResponse::AudioStatus {
                status: AudioPipelineStatus::idle(),
            },
            DaemonResponse::Error {
                message: "meeting archive failed at /private/path".to_string(),
            },
        ]);

        let error = daemon_end_session_with(&mut daemon, Duration::ZERO, test_mutation_fence())
            .await
            .unwrap_err();
        assert_eq!(error, "Bluey couldn't end the session. Try again.");
        assert!(!error.contains("/private/path"));
        assert_eq!(daemon.requests.len(), 2);
        assert!(matches!(
            &daemon.requests[1],
            DaemonRequest::MeetingEndBound { .. }
        ));
    }

    #[test]
    fn legacy_compatibility_addr_remains_stable() {
        assert_eq!(cue_core::ipc::DEFAULT_DAEMON_ADDR, "127.0.0.1:57321");
    }

    #[test]
    fn frontend_error_command_metadata_is_closed() {
        assert!(is_safe_frontend_command("list_sessions_v2"));
        assert!(!is_safe_frontend_command("list-sessions"));
        assert!(!is_safe_frontend_command("https://bluey.sh/?token=secret"));
        assert!(!is_safe_frontend_command(""));
        assert!(!is_safe_frontend_command(&"a".repeat(65)));
    }

    #[test]
    fn frontend_error_payload_rejects_raw_content_fields() {
        let payload: FrontendErrorPayload = serde_json::from_value(serde_json::json!({
            "source": "tauri_invoke",
            "category": "invoke_rejected",
            "command": "list_sessions_v2"
        }))
        .expect("closed metadata payload should deserialize");
        assert!(matches!(payload.source, FrontendErrorSource::TauriInvoke));
        assert!(matches!(
            payload.category,
            FrontendErrorCategory::InvokeRejected
        ));

        let raw_content = serde_json::from_value::<FrontendErrorPayload>(serde_json::json!({
            "source": "window_error",
            "category": "runtime_error",
            "message": "private transcript text",
            "url": "https://bluey.sh/?token=secret",
            "stack": "/Users/example/private.rs:42"
        }));
        assert!(raw_content.is_err());
    }

    #[test]
    fn test_session_switched_payload_serializes() {
        let payload = SessionSwitchedPayload {
            id: Some("abc-123".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("abc-123"));
    }

    #[test]
    fn test_session_switched_payload_none() {
        let payload = SessionSwitchedPayload { id: None };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("null"));
    }

    #[test]
    fn test_get_app_version_not_empty() {
        let version = get_app_version();
        assert!(!version.is_empty());
    }

    #[test]
    fn context_mode_status_uses_authoritative_daemon_state() {
        let mut state = cue_core::DaemonState::new(42);
        state.screen_capture_active = true;
        state.screen_capture_interval_secs = Some(30);
        state.screen_capture_generation = 4;
        state.context_items = 7;

        let payload = context_mode_status_from_response(DaemonResponse::Status { state })
            .expect("status payload");
        assert_eq!(
            payload,
            ContextModeStatusPayload {
                active: true,
                interval_secs: Some(30),
                context_items: 7,
                capture_generation: 4,
            }
        );
    }

    #[test]
    fn context_mode_status_surfaces_daemon_errors() {
        let error = context_mode_status_from_response(DaemonResponse::Error {
            message: "capture denied".to_string(),
        })
        .expect_err("daemon error");
        assert_eq!(error, "capture denied");
    }

    #[test]
    fn context_item_summary_exposes_status_without_local_paths_or_content() {
        let artifact = cue_core::ContextArtifact::new(
            cue_core::ContextKind::Text,
            "/private/bluey/page-context/secret.txt",
            "ChatGPT · Release planning",
            Some("Context mode observation. Changed readable page text.".to_string()),
            Some(512),
        )
        .with_text_preview("private page content");
        let payload = context_item_summary_payload(artifact);
        assert_eq!(payload.title, "ChatGPT · Release planning");
        assert_eq!(payload.kind, "text");
        assert_eq!(payload.processing_status, "ready");
        assert_eq!(
            payload.answer_context_role,
            cue_core::AnswerContextRole::Other
        );
        assert!(payload.context_mode_observation);
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"answer_context_role\":\"other\""));
        assert!(!json.contains("/private/bluey"));
        assert!(!json.contains("private page content"));
    }

    #[test]
    fn confirmed_story_role_requires_an_affirmative_dashboard_action() {
        assert!(validate_context_role_confirmation(
            cue_core::AnswerContextRole::UserConfirmedStory,
            false
        )
        .is_err());
        assert!(validate_context_role_confirmation(
            cue_core::AnswerContextRole::UserConfirmedStory,
            true
        )
        .is_ok());
        assert!(
            validate_context_role_confirmation(cue_core::AnswerContextRole::Other, false).is_ok()
        );
    }

    #[test]
    fn context_watch_update_is_semantic_first_bounded_and_normalized() {
        let mut settings = cue_core::CueSettings::default();
        apply_context_watch_settings(
            &mut settings,
            ContextWatchSettingsPayload {
                semantic_first: false,
                screenshot_fallback: true,
                interval_secs: 1,
                max_local_items: usize::MAX,
                excluded_apps: vec![
                    "  Google Chrome  ".to_string(),
                    "google chrome".to_string(),
                    String::new(),
                ],
                excluded_domains: vec![
                    " Accounts.Example.COM ".to_string(),
                    "accounts.example.com".to_string(),
                ],
            },
        );
        settings.touch();

        assert!(settings.context_watch.semantic_first);
        assert!(settings.context_watch.screenshot_fallback);
        assert_eq!(settings.context_watch.interval_secs, 3);
        assert_eq!(settings.context_watch.max_local_items, 500);
        assert_eq!(
            settings.context_watch.excluded_apps,
            vec!["google chrome".to_string()]
        );
        assert_eq!(
            settings.context_watch.excluded_domains,
            vec!["accounts.example.com".to_string()]
        );
        assert_eq!(
            context_watch_settings_payload(&settings.context_watch),
            ContextWatchSettingsPayload {
                semantic_first: true,
                screenshot_fallback: true,
                interval_secs: 3,
                max_local_items: 500,
                excluded_apps: vec!["google chrome".to_string()],
                excluded_domains: vec!["accounts.example.com".to_string()],
            }
        );
    }

    #[test]
    fn test_privacy_settings_command_microphone() {
        let result = privacy_settings_command("microphone");
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            assert!(result.is_ok());
            let (program, args) = result.unwrap();
            #[cfg(target_os = "macos")]
            {
                assert_eq!(program, "open");
                assert_eq!(args.len(), 1);
                assert!(args[0].contains("Privacy_Microphone"));
            }
            #[cfg(target_os = "windows")]
            {
                assert_eq!(program, "cmd");
                assert!(args.contains(&"ms-settings:privacy-microphone".to_string()));
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_privacy_settings_command_system() {
        let result = privacy_settings_command("system");
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            assert!(result.is_ok());
            let (program, args) = result.unwrap();
            #[cfg(target_os = "macos")]
            {
                assert_eq!(program, "open");
                assert_eq!(args.len(), 1);
                assert!(args[0].contains("Privacy_ScreenCapture"));
            }
            #[cfg(target_os = "windows")]
            {
                assert_eq!(program, "cmd");
                assert!(args.contains(&"ms-settings:privacy-microphone".to_string()));
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_permission_denied_payload_serializes() {
        let payload = PermissionDeniedPayload {
            source: "microphone".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("microphone"));
    }

    #[test]
    fn permission_poll_requires_successful_source_capture_before_allowed() {
        let mut status = active_audio_status();
        let initial = audio_permission_poll_payload(&status);
        assert_eq!(
            initial.sources,
            vec![
                AudioPermissionSourcePayload {
                    source: "microphone".to_string(),
                    verification: AudioPermissionVerification::NeedsListening,
                },
                AudioPermissionSourcePayload {
                    source: "system".to_string(),
                    verification: AudioPermissionVerification::NeedsListening,
                },
            ]
        );

        status.capture.microphone.chunks_captured = 1;
        status.capture.microphone.last_sequence = Some(1);
        assert_eq!(
            audio_permission_verification(&status, AudioSourceKind::Microphone),
            AudioPermissionVerification::Allowed
        );
        assert_eq!(
            audio_permission_verification(&status, AudioSourceKind::System),
            AudioPermissionVerification::NeedsListening
        );
    }

    #[test]
    fn permission_denial_wins_over_earlier_success() {
        let mut status = active_audio_status();
        status.capture.system.chunks_captured = 2;
        status.capture.system.last_sequence = Some(2);
        status.capture.permission_denied_source = Some(AudioSourceKind::System);
        assert_eq!(
            audio_permission_verification(&status, AudioSourceKind::System),
            AudioPermissionVerification::Denied
        );

        status.capture.permission_denied_source = None;
        status.capture.system.last_error = Some("Screen Recording permission denied".to_string());
        assert_eq!(
            audio_permission_verification(&status, AudioSourceKind::System),
            AudioPermissionVerification::Denied
        );
    }

    #[test]
    fn streamed_response_payload_always_names_its_source_session() {
        let payload = CueResponseChunkPayload {
            response_id: "response-1".to_string(),
            source_session_id: "session-b".to_string(),
            kind: "answer".to_string(),
            partial_text: "Hello".to_string(),
            finished: false,
            cost_cents: None,
            balance_cents_after: None,
            provider: None,
            model: None,
            cost_label: None,
            artifact_type: None,
            artifact_body: None,
            artifact_confidence: None,
            router_meta: None,
            replace_body: None,
        };
        let json = serde_json::to_value(payload).expect("serialize chunk");
        assert_eq!(json["source_session_id"], "session-b");
    }
}

// ===== Phase 3 Round 7: Live Transcript =====

/// Payload matching the  Tauri event shape.
#[derive(Clone, Serialize)]
pub struct LiveTranscriptPayload {
    pub index: usize,
    pub session_id: String,
    pub source: String,
    pub text: String,
    pub is_final: bool,
    pub speaker: Option<u8>,
    pub ts_ms: u64,
}

/// Return recent transcript segments from the daemon's active meeting file.
/// The UI calls this on mount for catch-up, and the background poller uses
/// it to detect new segments and emit  Tauri events.
#[tauri::command]
pub fn get_live_transcripts(
    since_index: usize,
    app: AppHandle,
) -> Result<Vec<LiveTranscriptPayload>, String> {
    let owner_guard = capture_dashboard_owner_guard(&app)?;
    owner_guard.with_current(&app, || {
        let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
        let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;
        let Some(meeting) = store.load_active().map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        if !owner_guard
            .identity
            .owner
            .owns_meeting(meeting.owner_account_id.as_deref())
        {
            return Ok(Vec::new());
        }
        let session_id = meeting.id.to_string();
        Ok(meeting
            .transcript
            .iter()
            .enumerate()
            .skip(since_index)
            .map(|(i, seg)| {
                let source = match seg.speaker {
                    cue_core::Speaker::System => "system",
                    cue_core::Speaker::User => "microphone",
                    _ => "unknown",
                };
                LiveTranscriptPayload {
                    index: i,
                    session_id: session_id.clone(),
                    source: source.to_string(),
                    text: seg.text.clone(),
                    is_final: seg.is_final,
                    speaker: None,
                    ts_ms: seg.created_at.parse::<u64>().unwrap_or(0),
                }
            })
            .collect())
    })
}

// ===== Phase 3 Round 9: Mouse Passthrough Toggle =====

/// Set overlay mouse passthrough state and persist it.
#[tauri::command]
pub fn set_mouse_passthrough(enabled: bool, db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.save_setting(
        "overlay_passthrough",
        if enabled { "true" } else { "false" },
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Get the current overlay mouse passthrough state.
#[tauri::command]
pub fn get_mouse_passthrough(db: State<DbState>) -> Result<bool, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    let val = db
        .load_setting("overlay_passthrough")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "true".to_string());
    Ok(val == "true")
}

// ===== Phase 3 Round 9: User-Rebindable Keybinds =====

pub(crate) const DEFAULT_LISTENING_SHORTCUT: &str = "Ctrl+Alt+L";

pub struct ListeningShortcutState(pub String);

/// A keybind entry returned to the frontend.
#[derive(Clone, Serialize)]
pub struct KeybindEntry {
    pub action: String,
    pub accelerator: String,
}

/// Default keybinds for known actions.
fn default_keybinds() -> Vec<(&'static str, &'static str)> {
    if cfg!(target_os = "macos") {
        vec![
            ("toggle_listening", DEFAULT_LISTENING_SHORTCUT),
            ("push_to_talk", "CmdOrCtrl+Shift+P"),
            ("toggle_overlay", "CmdOrCtrl+Shift+H"),
            ("toggle_dashboard", "CmdOrCtrl+Shift+D"),
        ]
    } else {
        vec![
            ("toggle_listening", DEFAULT_LISTENING_SHORTCUT),
            ("push_to_talk", "Ctrl+Shift+P"),
            ("toggle_overlay", "Ctrl+Shift+H"),
            ("toggle_dashboard", "Ctrl+Shift+D"),
        ]
    }
}

pub(crate) fn listening_shortcut_accelerator(db_state: &DbState) -> String {
    let candidate = db_state.0.lock().ok().and_then(|db| {
        if let Err(error) = db.ensure_keybinds_table() {
            tracing::warn!(%error, "failed to prepare listening shortcut settings");
            return None;
        }
        match db.load_keybind("toggle_listening") {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, "failed to load listening shortcut");
                None
            }
        }
    });

    candidate
        .filter(|accelerator| {
            accelerator
                .parse::<tauri_plugin_global_shortcut::Shortcut>()
                .is_ok()
        })
        .unwrap_or_else(|| DEFAULT_LISTENING_SHORTCUT.to_string())
}

#[cfg(target_os = "macos")]
fn listening_shortcut_label(accelerator: &str) -> String {
    accelerator
        .split('+')
        .map(|part| match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "⌃".to_string(),
            "alt" | "option" => "⌥".to_string(),
            "shift" => "⇧".to_string(),
            "cmd" | "command" | "meta" | "super" | "cmdorctrl" => "⌘".to_string(),
            _ => part.trim().to_ascii_uppercase(),
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(not(target_os = "macos"))]
fn listening_shortcut_label(accelerator: &str) -> String {
    accelerator
        .split('+')
        .map(|part| {
            if part.trim().eq_ignore_ascii_case("cmdorctrl") {
                "Ctrl".to_string()
            } else {
                part.trim().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Return the startup listening shortcut as a platform-friendly display label.
#[tauri::command]
pub fn get_listening_shortcut(shortcut: State<ListeningShortcutState>) -> String {
    listening_shortcut_label(&shortcut.0)
}

/// List all keybinds (from DB, falling back to defaults).
#[tauri::command]
pub fn list_keybinds(db: State<DbState>) -> Result<Vec<KeybindEntry>, String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.ensure_keybinds_table().map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for (action, default_accel) in default_keybinds() {
        let accel = db
            .load_keybind(action)
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| default_accel.to_string());
        entries.push(KeybindEntry {
            action: action.to_string(),
            accelerator: accel,
        });
    }
    Ok(entries)
}

/// Set a keybind for an action. Validates the accelerator string.
/// A changed listening shortcut takes effect after Bluey restarts.
#[tauri::command]
pub fn set_keybind(action: String, accelerator: String, db: State<DbState>) -> Result<(), String> {
    // Validate accelerator by attempting to parse
    accelerator
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("invalid accelerator: {e}"))?;
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.ensure_keybinds_table().map_err(|e| e.to_string())?;
    db.save_keybind(&action, &accelerator)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Reset all keybinds to defaults.
#[tauri::command]
pub fn reset_keybinds(db: State<DbState>) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.ensure_keybinds_table().map_err(|e| e.to_string())?;
    db.reset_keybinds().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod listening_shortcut_tests {
    use super::*;

    fn shortcut_state(accelerator: Option<&str>) -> DbState {
        let db = cue_daemon::db::Database::open(":memory:").expect("open test database");
        db.ensure_keybinds_table().expect("create keybind table");
        if let Some(accelerator) = accelerator {
            db.save_keybind("toggle_listening", accelerator)
                .expect("save listening shortcut");
        }
        DbState(Mutex::new(db))
    }

    #[test]
    fn listening_shortcut_uses_valid_persisted_value() {
        let state = shortcut_state(Some("CmdOrCtrl+Shift+K"));
        assert_eq!(listening_shortcut_accelerator(&state), "CmdOrCtrl+Shift+K");
    }

    #[test]
    fn listening_shortcut_rejects_invalid_persisted_value() {
        let state = shortcut_state(Some(""));
        assert_eq!(
            listening_shortcut_accelerator(&state),
            DEFAULT_LISTENING_SHORTCUT
        );
    }

    #[test]
    fn listening_shortcut_default_and_label_share_one_accelerator() {
        let default = default_keybinds()
            .into_iter()
            .find(|(action, _)| *action == "toggle_listening")
            .expect("listening keybind default")
            .1;
        assert_eq!(default, DEFAULT_LISTENING_SHORTCUT);
        #[cfg(target_os = "macos")]
        assert_eq!(listening_shortcut_label(default), "⌃⌥L");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(listening_shortcut_label(default), "Ctrl+Alt+L");
    }
}

// ===== Phase 3 Round 10: Cmd+Shift+A → request_cue =====

/// Payload emitted per streaming chunk on `cue_response_chunk`.
#[derive(Clone, Serialize)]
pub struct CueResponseChunkPayload {
    pub response_id: String,
    pub source_session_id: String,
    pub kind: String,
    pub partial_text: String,
    pub finished: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents_after: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_confidence: Option<f32>,
    /// Auto Router metadata (Bluey Auto). Optional because future managed
    /// routing may attach more fields; for v0.1 we emit task_type, lane, and
    /// confidence on the FIRST chunk and reuse the same payload schema for
    /// subsequent chunks (router_meta = None on those).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub router_meta: Option<RouterMeta>,
    /// R14.4: when true, the UI replaces the entire card body with
    /// `partial_text` instead of appending. Used by the SpeculativeRouter
    /// Deep lane Final chunk so the draft -> final transition is a clean
    /// swap rather than a `\"[refined]\n...\"` concatenation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_body: Option<bool>,
}

/// Subset of cue_router::TaskClassification + ProviderRoute exposed to the UI.
#[derive(Clone, Serialize)]
pub struct RouterMeta {
    /// Task type the heuristic classifier picked (general / code /
    /// system_design / meeting / writing / vision).
    pub task_type: String,
    /// Latency lane the policy picked (instant / balanced / deep).
    pub latency_lane: String,
    /// Provider lane the policy resolved to (instant / balanced / deep /
    /// vision / local).
    pub provider_lane: String,
    /// Provider name dispatched to (e.g. "openai", "anthropic", "ollama").
    pub provider_name: String,
    /// Model dispatched to.
    pub model: String,
    /// Heuristic classifier confidence in [0.0, 1.0].
    pub confidence: f32,
}

/// Same classification logic as `classify_for_router` but returns the raw
/// TaskClassification (used by the speculative path which needs the actual
/// classification object to drive routing decisions).
fn classify_only_for_router(
    prompt: &str,
    has_transcript: bool,
    has_screenshot: bool,
) -> cue_router::TaskClassification {
    use cue_router::{ClassifierInput, HeuristicClassifier, TaskClassifier};
    let classifier = HeuristicClassifier::new();
    let input = ClassifierInput {
        prompt,
        has_transcript,
        has_page: false,
        file_attachment_count: 0,
        has_screenshot,
    };
    futures::executor::block_on(classifier.classify(&input))
}

fn classify_for_router(prompt: &str, has_transcript: bool, has_screenshot: bool) -> RouterMeta {
    use cue_router::{
        AutoRouter, ClassifierInput, HeuristicClassifier, RouteOptions, RoutingPolicy,
        StaticPolicy, TaskClassifier,
    };
    use std::sync::Arc;

    let classifier: Arc<dyn TaskClassifier> = Arc::new(HeuristicClassifier::new());
    let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
    let router = AutoRouter::new(classifier, policy);

    let local_only = std::env::var("BLUEY_LOCAL_ONLY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let input = ClassifierInput {
        prompt,
        has_transcript,
        has_page: false,
        file_attachment_count: 0,
        has_screenshot,
    };

    // Synchronous wrapper: AutoRouter::route is async only because future
    // managed classifiers may RPC. The heuristic path is in-process and
    // returns immediately; we block the async runtime briefly.
    let routed = futures::executor::block_on(router.route(&input, RouteOptions { local_only }));

    RouterMeta {
        task_type: format!("{:?}", routed.classification.task_type).to_lowercase(),
        latency_lane: format!("{:?}", routed.classification.latency_lane).to_lowercase(),
        provider_lane: format!("{:?}", routed.route.lane).to_lowercase(),
        provider_name: routed.route.provider_name.clone(),
        model: routed.route.model.clone(),
        confidence: routed.classification.confidence,
    }
}

/// Try the speculative routing path. Returns:
///   Ok(Some((text, metadata))) if speculation succeeded and produced a final answer.
///   Ok(None)                   if speculation is OFF or unconfigured (caller should
///                              fall back to the legacy AnswerLlm/WhatToAnswerLlm path).
///   Err(e)                     if speculation was ON and failed mid-stream.
///
/// Forwards every chunk through the existing `cue_response_chunk` Tauri event
/// so the dashboard UI does not need to know the answer was speculative.
#[allow(clippy::too_many_arguments)]
async fn try_speculative_dispatch(
    user_text: &str,
    session_id: &str,
    system_prompt: &str,
    kind: &str,
    response_id: &str,
    classification: cue_router::TaskClassification,
    router_meta: RouterMeta,
    registry: ProviderRegistry,
    app: tauri::AppHandle,
    owner_guard: DashboardOwnerGuard,
    stream_token: DashboardOwnerStreamToken,
) -> Result<Option<(String, LlmResponseMetadata)>, String> {
    use cue_router::{
        policy::StaticPolicy, speculative::SpeculativeChunk, RoutingPolicy, SpeculativeRouter,
    };
    use futures::stream::StreamExt;
    use std::sync::Arc;

    // Bluey Auto routing is default-ON: classify once, pick the best lane,
    // and stream one visible answer card from that lane. Parallel
    // cheap-draft + deep-final replacement remains dev-gated because it made
    // the customer UI feel jumpy during live questions.
    let speculative_off = std::env::var("BLUEY_SPECULATIVE_ROUTING")
        .map(|v| {
            let v = v.trim();
            v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(false);
    if speculative_off || registry.is_empty() {
        return Ok(None);
    }
    let parallel_drafts = std::env::var("BLUEY_PARALLEL_DRAFTS")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on")
        })
        .unwrap_or(false);

    // Codex Stage 9 round-2 Blocker 2: lane-correct policy selection.
    // StaticPolicy emits openai/anthropic provider names; a managed
    // registry contains only bluey-managed-* names. Detect from the
    // registry contents and pick ManagedPolicy when managed; otherwise
    // StaticPolicy for legacy BYOK.
    let policy: Arc<dyn RoutingPolicy> = if registry.is_managed_only() {
        Arc::new(cue_router::ManagedPolicy::new())
    } else {
        Arc::new(StaticPolicy::defaults())
    };
    let provider: Arc<dyn cue_router::speculative::SpeculativeProvider> = Arc::new(registry);
    // Normal product mode is single visible stream from the selected lane.
    // `BLUEY_PARALLEL_DRAFTS=1` is retained for latency experiments only.
    let router = SpeculativeRouter::new(policy, provider, parallel_drafts);

    let req = cue_llm::LlmRequest {
        system: system_prompt.to_string(),
        user: user_text.to_string(),
        session_id: Some(session_id.to_string()),
        max_tokens: None,
        temperature: None,
        reasoning_effort: None,
        thinking_budget_tokens: None,
        // Codex Stage 9b: thread the daemon's response_id as the
        // stable logical request id so retries / failover hit the
        // server idempotency cache instead of double-charging.
        request_id: Some(response_id.to_string()),
        image_data_urls: Vec::new(),
        context: Vec::new(),
    };

    owner_guard.ensure_current(&app)?;
    stream_token.with_current(&owner_guard, &app, || Ok(()))?;
    let mut dispatch_cancelled = stream_token.receiver();
    let stream = tokio::select! {
        stream = router.run(&classification, req) => stream.map_err(|e| e.to_string())?,
        _ = wait_for_owner_stream_cancellation(&mut dispatch_cancelled) => {
            return Err(account_changed_message());
        }
    };
    let mut stream = Box::pin(stream);
    let mut stream_cancelled = stream_token.receiver();

    let mut accumulated_draft = String::new();
    let mut final_text: Option<String> = None;
    let mut metadata = LlmResponseMetadata::default();
    // Keep only a count for fallback. Raw provider messages can contain
    // response bodies, URLs, tokens, local paths, or user-controlled text and
    // must never enter dashboard logs.
    let mut lane_error_count = 0usize;
    let mut emitted_meta = false;

    loop {
        let chunk = tokio::select! {
            chunk = stream.next() => chunk,
            _ = wait_for_owner_stream_cancellation(&mut stream_cancelled) => {
                return Err(account_changed_message());
            }
        };
        let Some(chunk) = chunk else {
            break;
        };
        match chunk {
            SpeculativeChunk::Draft {
                text,
                finished,
                cost: chunk_cost,
                cost_label,
                artifact,
            } => {
                if let Some(chunk_cost) = chunk_cost.as_ref() {
                    merge_cost_metadata(&mut metadata.cost, chunk_cost.clone());
                }
                if cost_label.is_some() {
                    metadata.cost_label = cost_label.clone();
                }
                if artifact.is_some() {
                    metadata.artifact = artifact.clone();
                }
                accumulated_draft.push_str(&text);
                let meta_for_chunk = if !emitted_meta {
                    emitted_meta = true;
                    Some(router_meta.clone())
                } else {
                    None
                };
                stream_token.with_current(&owner_guard, &app, || {
                    let _ = app.emit(
                        "cue_response_chunk",
                        CueResponseChunkPayload {
                            response_id: response_id.to_string(),
                            source_session_id: session_id.to_string(),
                            kind: kind.to_string(),
                            partial_text: text,
                            finished,
                            cost_cents: chunk_cost.as_ref().map(|cost| cost.cost_cents),
                            balance_cents_after: chunk_cost
                                .as_ref()
                                .and_then(|cost| cost.balance_cents_after),
                            provider: chunk_cost.as_ref().map(|cost| cost.provider.clone()),
                            model: chunk_cost.as_ref().map(|cost| cost.model.clone()),
                            cost_label,
                            artifact_type: artifact_type(&artifact),
                            artifact_body: artifact_body(&artifact),
                            artifact_confidence: artifact_confidence(&artifact),
                            router_meta: meta_for_chunk,
                            replace_body: None,
                        },
                    );
                    Ok(())
                })?;
            }
            SpeculativeChunk::Final {
                text,
                cost: chunk_cost,
                cost_label,
                artifact,
            } => {
                if let Some(chunk_cost) = chunk_cost.as_ref() {
                    merge_cost_metadata(&mut metadata.cost, chunk_cost.clone());
                }
                if cost_label.is_some() {
                    metadata.cost_label = cost_label.clone();
                }
                if artifact.is_some() {
                    metadata.artifact = artifact.clone();
                }
                stream_token.with_current(&owner_guard, &app, || {
                    let _ = app.emit(
                        "cue_response_chunk",
                        CueResponseChunkPayload {
                            response_id: response_id.to_string(),
                            source_session_id: session_id.to_string(),
                            kind: kind.to_string(),
                            partial_text: text.clone(),
                            finished: true,
                            cost_cents: chunk_cost.as_ref().map(|cost| cost.cost_cents),
                            balance_cents_after: chunk_cost
                                .as_ref()
                                .and_then(|cost| cost.balance_cents_after),
                            provider: chunk_cost.as_ref().map(|cost| cost.provider.clone()),
                            model: chunk_cost.as_ref().map(|cost| cost.model.clone()),
                            cost_label,
                            artifact_type: artifact_type(&artifact),
                            artifact_body: artifact_body(&artifact),
                            artifact_confidence: artifact_confidence(&artifact),
                            router_meta: None,
                            replace_body: Some(true),
                        },
                    );
                    Ok(())
                })?;
                final_text = Some(text);
            }
            SpeculativeChunk::Error { lane, .. } => {
                tracing::warn!(
                    lane,
                    error_category = "provider_lane",
                    "speculative router lane error"
                );
                lane_error_count = lane_error_count.saturating_add(1);
                // Non-fatal individually: keep collecting the other lane.
            }
        }
    }

    // Codex review S9 round-3 blocker: actually use the error count. If
    // every lane errored AND no text was produced, return Ok(None) so
    // the caller falls back to the legacy single-shot path. Returning
    // empty Ok(Some("")) made try_speculative_dispatch silently
    // persist an empty cue card.
    let resolved = final_text.unwrap_or(accumulated_draft);
    let trimmed = resolved.trim().to_string();
    if trimmed.is_empty() && lane_error_count > 0 {
        tracing::warn!(
            lane_error_count,
            "speculative dispatch: every lane errored; falling back to legacy"
        );
        return Ok(None);
    }
    if trimmed.is_empty() {
        // No errors AND no text — nothing to render either way.
        return Ok(None);
    }
    Ok(Some((trimmed, metadata)))
}

fn merge_cost_metadata(
    existing: &mut Option<cue_llm::LlmCostMetadata>,
    next: cue_llm::LlmCostMetadata,
) {
    match existing {
        Some(current) => {
            if current.provider != next.provider {
                current.provider = format!("{}, {}", current.provider, next.provider);
            }
            if current.model != next.model {
                current.model = format!("{}, {}", current.model, next.model);
            }
            current.input_tokens = current.input_tokens.saturating_add(next.input_tokens);
            current.output_tokens = current.output_tokens.saturating_add(next.output_tokens);
            current.cost_cents = current.cost_cents.saturating_add(next.cost_cents);
            current.balance_cents_after = next.balance_cents_after.or(current.balance_cents_after);
            current.trial_seconds_remaining = next
                .trial_seconds_remaining
                .or(current.trial_seconds_remaining);
        }
        None => *existing = Some(next),
    }
}

#[derive(Debug, Clone, Default)]
struct LlmResponseMetadata {
    cost: Option<cue_llm::LlmCostMetadata>,
    cost_label: Option<String>,
    artifact: Option<cue_llm::LlmArtifactMetadata>,
}

fn artifact_type(artifact: &Option<cue_llm::LlmArtifactMetadata>) -> Option<String> {
    artifact
        .as_ref()
        .map(|artifact| artifact.artifact_type.clone())
}

fn artifact_body(artifact: &Option<cue_llm::LlmArtifactMetadata>) -> Option<String> {
    artifact.as_ref().map(|artifact| artifact.body.clone())
}

fn artifact_confidence(artifact: &Option<cue_llm::LlmArtifactMetadata>) -> Option<f32> {
    artifact.as_ref().and_then(|artifact| artifact.confidence)
}

/// Trigger a cue response. If a recent question is detected in the transcript
/// (last ~30s), runs AnswerLlm; otherwise runs WhatToAnswerLlm (suggestion).
/// Emits `cue_response_chunk` per streaming chunk, then `cue_response` on completion.
#[tauri::command]
pub async fn request_cue(
    kind: String,
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<String, String> {
    use cue_daemon::llm::{ends_with_question, AnswerLlm, WhatToAnswerLlm};

    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let meeting = owner_guard.with_current(&app, || {
        let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
        let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;
        let meeting = store
            .load_active()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no active session".to_string())?;
        if !owner_guard
            .identity
            .owner
            .owns_meeting(meeting.owner_account_id.as_deref())
        {
            return Err("no active session".to_string());
        }
        Ok(meeting)
    })?;

    let session_id = meeting.id.to_string();

    // Recent transcript text (last ~30s worth, approx last 10 segments).
    let recent: String = meeting
        .transcript
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if recent.trim().is_empty() {
        return Err("no recent transcript to analyze".to_string());
    }

    let managed_client = captured_managed_llm_client(&app, &owner_guard, &trace_id)?;
    let llm = build_llm_provider_from_env(&db, managed_client.clone())
        .ok_or_else(|| "no LLM provider configured".to_string())?;
    let stream_fence = DashboardOwnerStreamFence::start(&app, &owner_guard)?;

    // Generate response_id up-front so chunks and final event share it.
    let response_id = Uuid::new_v4().to_string();

    // Auto Router classification: emit on the first chunk so the UI can render
    // a lane badge before the answer text starts streaming.
    let router_meta = classify_for_router(&recent, true, false);
    let emitted_meta = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Bluey Auto path. **Default ON**: classify, route, and stream one visible
    // answer from the selected lane. Disable explicitly with
    // BLUEY_SPECULATIVE_ROUTING=0 / false / off. The older parallel
    // draft+deep experiment is retained behind BLUEY_PARALLEL_DRAFTS=1 only.
    // When routing is OFF or no providers are configured, fall through to the
    // legacy AnswerLlm / WhatToAnswerLlm path — zero regression.
    {
        use cue_daemon::llm::{answer as answer_mod, suggest as suggest_mod};
        let registry = ProviderRegistry::from_env_and_secrets(&db, managed_client.clone());
        let classification = classify_only_for_router(&recent, true, false);
        let is_question = kind == "answer" && ends_with_question(&recent);
        let user_text = if is_question {
            recent
                .rsplit('.')
                .find(|s| s.trim().ends_with('?'))
                .unwrap_or(&recent)
                .trim()
                .to_string()
        } else {
            recent.clone()
        };
        let system_prompt = if is_question {
            answer_mod::SYSTEM_PROMPT
        } else {
            suggest_mod::SYSTEM_PROMPT
        };
        let kind_str = if is_question { "answer" } else { "suggestion" };
        if let Some((text, response_metadata)) = try_speculative_dispatch(
            &user_text,
            &session_id,
            system_prompt,
            kind_str,
            &response_id,
            classification,
            router_meta.clone(),
            registry,
            app.clone(),
            owner_guard.clone(),
            stream_fence.token(),
        )
        .await
        .map_err(|e| e.to_string())?
        {
            let cue_resp = cue_daemon::llm::CueResponse::new(
                kind_str,
                text.clone(),
                &session_id,
                Some(recent.clone()),
            );
            let mut cue_resp = cue_resp;
            cue_resp.id = response_id.clone();
            cue_resp = cue_resp.with_llm_metadata(
                response_metadata.cost.as_ref(),
                response_metadata.cost_label.as_deref(),
                response_metadata.artifact.as_ref(),
            );
            return owner_guard.with_current(&app, || {
                persist_cue_response_for_owner(&db, &owner_guard.identity.owner, &cue_resp)?;
                let _ = app.emit("cue_response", &cue_resp);
                Ok(text)
            });
        }
    }

    // Detect question in recent transcript and dispatch with streaming.
    let emitted_meta_a = emitted_meta.clone();
    let emitted_meta_b = emitted_meta.clone();
    let router_meta_a = router_meta.clone();
    let router_meta_b = router_meta.clone();
    let source_session_id_a = session_id.clone();
    let source_session_id_b = session_id.clone();
    let cue_resp = if kind == "answer" && ends_with_question(&recent) {
        let question = recent
            .rsplit('.')
            .find(|s| s.trim().ends_with('?'))
            .unwrap_or(&recent)
            .trim();
        let app2 = app.clone();
        let owner_guard2 = owner_guard.clone();
        let stream_token = stream_fence.token();
        let chunk_token = stream_token.clone();
        let mut cancelled = stream_token.receiver();
        let rid = response_id.clone();
        owner_guard.ensure_current(&app)?;
        let response = tokio::select! {
            response = AnswerLlm.run_streaming(
                question,
                &session_id,
                llm.as_ref(),
                |partial, finished| {
                let meta_for_chunk =
                    if !emitted_meta_a.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        Some(router_meta_a.clone())
                    } else {
                        None
                    };
                let _ = chunk_token.with_current(&owner_guard2, &app2, || {
                    let _ = app2.emit(
                        "cue_response_chunk",
                        CueResponseChunkPayload {
                        response_id: rid.clone(),
                        source_session_id: source_session_id_a.clone(),
                        kind: "answer".to_string(),
                        partial_text: partial.to_string(),
                        finished,
                        cost_cents: None,
                        balance_cents_after: None,
                        provider: None,
                        model: None,
                        cost_label: None,
                        artifact_type: None,
                        artifact_body: None,
                        artifact_confidence: None,
                        router_meta: meta_for_chunk,
                        replace_body: None,
                        },
                    );
                    Ok(())
                });
            }) => response,
            _ = wait_for_owner_stream_cancellation(&mut cancelled) => {
                return Err(account_changed_message());
            }
        };
        response.map_err(|e| e.to_string())?
    } else {
        let app2 = app.clone();
        let owner_guard2 = owner_guard.clone();
        let stream_token = stream_fence.token();
        let chunk_token = stream_token.clone();
        let mut cancelled = stream_token.receiver();
        let rid = response_id.clone();
        owner_guard.ensure_current(&app)?;
        let response = tokio::select! {
            response = WhatToAnswerLlm.run_streaming(
                &recent,
                &session_id,
                llm.as_ref(),
                |partial, finished| {
                let meta_for_chunk =
                    if !emitted_meta_b.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        Some(router_meta_b.clone())
                    } else {
                        None
                    };
                let _ = chunk_token.with_current(&owner_guard2, &app2, || {
                    let _ = app2.emit(
                        "cue_response_chunk",
                        CueResponseChunkPayload {
                        response_id: rid.clone(),
                        source_session_id: source_session_id_b.clone(),
                        kind: "suggestion".to_string(),
                        partial_text: partial.to_string(),
                        finished,
                        cost_cents: None,
                        balance_cents_after: None,
                        provider: None,
                        model: None,
                        cost_label: None,
                        artifact_type: None,
                        artifact_body: None,
                        artifact_confidence: None,
                        router_meta: meta_for_chunk,
                        replace_body: None,
                        },
                    );
                    Ok(())
                });
            }) => response,
            _ = wait_for_owner_stream_cancellation(&mut cancelled) => {
                return Err(account_changed_message());
            }
        };
        response.map_err(|e| e.to_string())?
    };

    // Override the CueResponse id with our pre-generated response_id for consistency.
    let mut cue_resp = cue_resp;
    cue_resp.id = response_id;

    owner_guard.with_current(&app, || {
        persist_cue_response_for_owner(&db, &owner_guard.identity.owner, &cue_resp)?;
        let _ = app.emit("cue_response", &cue_resp);
        Ok(cue_resp.text.clone())
    })
}

/// Auto-recap: runs RecapLlm on a session's full transcript.
/// Emits `cue_response_chunk` per streaming chunk, then `cue_response` on completion.
#[tauri::command]
pub async fn auto_recap(
    session_id: String,
    db: State<'_, DbState>,
    app: AppHandle,
) -> Result<String, String> {
    use cue_daemon::llm::RecapLlm;

    let owner_guard = capture_dashboard_owner_guard(&app)?;
    let trace_id = dashboard_trace_id();
    let meeting_id = Uuid::parse_str(&session_id).map_err(|_| "session not found".to_string())?;
    let meeting = owner_guard.with_current(&app, || {
        let paths = cue_core::app_paths::AppPaths::discover().map_err(|e| e.to_string())?;
        let store = cue_daemon::storage::MeetingStore::new(&paths).map_err(|e| e.to_string())?;
        let meeting = store
            .load_by_id(meeting_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        if !owner_guard
            .identity
            .owner
            .owns_meeting(meeting.owner_account_id.as_deref())
        {
            return Err("session not found".to_string());
        }
        Ok(meeting)
    })?;

    let transcript: String = meeting
        .transcript
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if transcript.trim().is_empty() {
        return Err("empty transcript".to_string());
    }

    let managed_client = captured_managed_llm_client(&app, &owner_guard, &trace_id)?;
    let llm = match build_llm_provider_from_env(&db, managed_client) {
        Some(p) => p,
        None => {
            tracing::warn!("auto-recap skipped: no LLM provider configured");
            return Err("no LLM provider configured".to_string());
        }
    };
    let stream_fence = DashboardOwnerStreamFence::start(&app, &owner_guard)?;

    let response_id = Uuid::new_v4().to_string();
    let rid = response_id.clone();
    let stream_session_id = session_id.clone();
    let app2 = app.clone();
    let owner_guard2 = owner_guard.clone();
    let stream_token = stream_fence.token();
    let chunk_token = stream_token.clone();
    let mut cancelled = stream_token.receiver();

    owner_guard.ensure_current(&app)?;
    let cue_resp = tokio::select! {
        response = RecapLlm.run_streaming(
            &transcript,
            &session_id,
            llm.as_ref(),
            |partial, finished| {
                let _ = chunk_token.with_current(&owner_guard2, &app2, || {
                    let _ = app2.emit(
                        "cue_response_chunk",
                        CueResponseChunkPayload {
                        response_id: rid.clone(),
                        source_session_id: stream_session_id.clone(),
                        kind: "recap".to_string(),
                        partial_text: partial.to_string(),
                        finished,
                        cost_cents: None,
                        balance_cents_after: None,
                        provider: None,
                        model: None,
                        cost_label: None,
                        artifact_type: None,
                        artifact_body: None,
                        artifact_confidence: None,
                        router_meta: None, // recap is not yet routed via AutoRouter
                        replace_body: None,
                        },
                    );
                    Ok(())
                });
            },
        ) => response.map_err(|e| e.to_string())?,
        _ = wait_for_owner_stream_cancellation(&mut cancelled) => {
            return Err(account_changed_message());
        }
    };

    let mut cue_resp = cue_resp;
    cue_resp.id = response_id;

    owner_guard.with_current(&app, || {
        persist_cue_response_for_owner(&db, &owner_guard.identity.owner, &cue_resp)?;
        let _ = app.emit("cue_response", &cue_resp);
        Ok(cue_resp.text.clone())
    })
}

fn persist_cue_response_for_owner(
    db: &State<DbState>,
    owner: &DashboardOwner,
    resp: &cue_daemon::llm::CueResponse,
) -> Result<(), String> {
    let db = db.0.lock().map_err(|e| e.to_string())?;
    db.insert_cue_response_for_owner(
        owner.db_owner_id(),
        cue_daemon::db::NewCueResponse {
            id: &resp.id,
            session_id: &resp.source_session_id,
            kind: &resp.kind,
            text: &resp.text,
            source_text: resp.source_text.as_deref(),
            ts_ms: resp.ts_ms as i64,
            cost_cents: resp.cost_cents,
            balance_cents_after: resp.balance_cents_after,
            provider: resp.provider.as_deref(),
            model: resp.model.as_deref(),
            input_tokens: resp.input_tokens,
            output_tokens: resp.output_tokens,
            cost_label: resp.cost_label.as_deref(),
            artifact_type: resp.artifact_type.as_deref(),
            artifact_body: resp.artifact_body.as_deref(),
            artifact_confidence: resp.artifact_confidence,
        },
    )
    .map_err(|e| e.to_string())
}

/// Multi-provider registry: builds every LLM provider for which credentials
/// are configured (env var or opt-in saved dev secret) and exposes them by `LlmProvider::name()`.
///
/// This is what the SpeculativeRouter dispatches against. If the chosen route
/// targets a provider that is not in the registry, we fall back to whichever
/// provider IS available so a misconfigured Anthropic key does not block an
/// OpenAI-only setup from running the deep lane.
struct ProviderRegistry {
    providers: std::collections::HashMap<String, std::sync::Arc<dyn cue_llm::LlmProvider>>,
}

impl ProviderRegistry {
    fn from_env_and_secrets(
        _db: &tauri::State<DbState>,
        managed_client: Option<cue_cloud_client::CloudClient>,
    ) -> Self {
        use std::collections::HashMap;
        use std::sync::Arc;

        let mut providers: HashMap<String, Arc<dyn cue_llm::LlmProvider>> = HashMap::new();

        // Codex Stage 9a: managed-mode detection. If a Bluey account
        // token is in the local account store, register BlueyManagedProvider for
        // every cloud lane; bluey-server will pick the actual upstream
        // provider+model. Customer pays Bluey; Bluey owns the API keys.
        //
        // Legacy BYOK direct providers (OpenAI/Anthropic from env or saved
        // secrets) are gated behind BLUEY_DEV_BYOK=1 in debug/dev builds so
        // dev workflows still work while shipped customer binaries stay
        // managed-only.
        let managed_mode = managed_client.is_some();

        if managed_mode {
            tracing::info!("ProviderRegistry: managed mode active (account token found)");
            // One BlueyManagedProvider per cloud lane. The provider
            // name (bluey-managed-{lane}) matches what
            // cue_router::ManagedPolicy emits, so the registry lookup
            // dispatches correctly.
            if let Some(client) = managed_client {
                for lane in [
                    cue_llm::bluey_managed::ManagedLane::Instant,
                    cue_llm::bluey_managed::ManagedLane::Balanced,
                    cue_llm::bluey_managed::ManagedLane::Deep,
                    cue_llm::bluey_managed::ManagedLane::Vision,
                ] {
                    let provider: Arc<dyn cue_llm::LlmProvider> = Arc::new(
                        cue_llm::bluey_managed::BlueyManagedProvider::new(client.clone(), lane),
                    );
                    providers.insert(provider.name().to_string(), provider);
                }
            }
        }

        let allow_byok = dev_byok_enabled();

        if allow_byok {
            if let Some(key) = std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    allow_byok
                        .then(|| {
                            cue_daemon::secrets::load_api_key("llm_openai")
                                .ok()
                                .flatten()
                        })
                        .flatten()
                })
            {
                let provider: Arc<dyn cue_llm::LlmProvider> =
                    Arc::new(cue_llm::openai::OpenAiProvider::new(key));
                providers.insert("openai".to_string(), provider);
            }

            if let Some(key) = std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|k| !k.is_empty())
                .or_else(|| {
                    allow_byok
                        .then(|| {
                            cue_daemon::secrets::load_api_key("llm_anthropic")
                                .ok()
                                .flatten()
                        })
                        .flatten()
                })
            {
                let provider: Arc<dyn cue_llm::LlmProvider> =
                    Arc::new(cue_llm::anthropic::AnthropicProvider::new(key));
                providers.insert("anthropic".to_string(), provider);
            }
        }

        // Ollama is a developer-only local path. Production/customer Bluey
        // answers must route through bluey-server so provider credentials and
        // billing remain server-side.
        if allow_byok {
            if let Ok(host) = std::env::var("BLUEY_OLLAMA_HOST") {
                if !host.is_empty() {
                    if std::env::var("OLLAMA_BASE_URL").is_err() {
                        // SAFETY: this is the daemon process at request time.
                        unsafe {
                            std::env::set_var("OLLAMA_BASE_URL", &host);
                        }
                    }
                    let provider: Arc<dyn cue_llm::LlmProvider> =
                        Arc::new(cue_llm::ollama::OllamaProvider::new());
                    providers.insert("ollama".to_string(), provider);
                }
            }
        }

        Self { providers }
    }

    fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl ProviderRegistry {
    /// Codex Stage 9 round-2: true when every registered LLM provider is
    /// a `bluey-managed-*` name. Used to pick the right RoutingPolicy.
    pub fn is_managed_only(&self) -> bool {
        if self.providers.is_empty() {
            return false;
        }
        self.providers
            .keys()
            .all(|k| k.starts_with("bluey-managed-"))
    }
}

#[async_trait::async_trait]
impl cue_router::speculative::SpeculativeProvider for ProviderRegistry {
    async fn provider_for(
        &self,
        route: &cue_router::ProviderRoute,
    ) -> Result<std::sync::Arc<dyn cue_llm::LlmProvider>, cue_llm::LlmError> {
        if let Some(p) = self.providers.get(&route.provider_name) {
            return Ok(p.clone());
        }
        // Codex Stage 9 round-2 Blocker 2: NO arbitrary HashMap fallback.
        // Returning an arbitrary provider can silently misroute a Deep
        // classification to (e.g.) bluey-managed-instant. If the named
        // provider is missing it is a registry-construction bug; surface
        // it as an error so the caller falls back to legacy single-shot.
        Err(cue_llm::LlmError::Provider(format!(
            "no provider registered for route {} (managed_only={}, registered={:?})",
            route.provider_name,
            self.is_managed_only(),
            self.providers.keys().collect::<Vec<_>>()
        )))
    }
}

/// Build an LLM provider from environment variables or stored secrets.
///
/// Codex Stage 9 round-2 Blocker 1: managed-mode customers (Bluey
/// account token in local account store, no direct OPENAI_API_KEY) used to be
/// rejected here. We now check managed mode first and return a
/// BlueyManagedProvider bound to the Balanced lane as the legacy
/// single-shot fallback. The speculative path (`try_speculative_
/// dispatch`) picks per-lane providers separately.
fn build_llm_provider_from_env(
    _db: &State<DbState>,
    managed_client: Option<cue_cloud_client::CloudClient>,
) -> Option<Box<dyn cue_llm::LlmProvider>> {
    // Managed mode first: Bluey account tokens take priority over BYOK unless
    // a debug/dev build has BLUEY_DEV_BYOK=1 explicitly set (matching the
    // ProviderRegistry policy).
    let allow_byok = dev_byok_enabled();
    if !allow_byok {
        if let Some(client) = managed_client {
            return Some(Box::new(cue_llm::bluey_managed::BlueyManagedProvider::new(
                client,
                cue_llm::bluey_managed::ManagedLane::Balanced,
            )));
        }
        return None;
    }

    let openai_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            cue_daemon::secrets::load_api_key("llm_openai")
                .ok()
                .flatten()
        })?;
    Some(Box::new(cue_llm::openai::OpenAiProvider::new(openai_key)))
}

fn dev_byok_enabled() -> bool {
    cfg!(debug_assertions)
        && std::env::var("BLUEY_DEV_BYOK")
            .ok()
            .is_some_and(|value| value.trim() == "1")
}

// ─── R8 nit hardening tests (recheck rollout) ───────────────────────────────

#[cfg(test)]
mod r8_nit_tests {
    /// Char-safe last-4 masking matches the new load_stt_api_key behavior:
    /// must NOT panic on multi-byte UTF-8 trailing bytes.
    fn mask_last_four(s: &str) -> String {
        let total = s.chars().count();
        if total <= 4 {
            "****".to_string()
        } else {
            let suffix: String = s.chars().skip(total - 4).collect();
            format!("****{suffix}")
        }
    }

    #[test]
    fn mask_short_key() {
        assert_eq!(mask_last_four("ab"), "****");
        assert_eq!(mask_last_four("abcd"), "****");
    }

    #[test]
    fn mask_ascii_key() {
        assert_eq!(mask_last_four("sk-abcd1234"), "****1234");
    }

    #[test]
    fn mask_multibyte_key_does_not_panic() {
        // Trailing 4 chars are emoji + ASCII — would have panicked under
        // byte-slicing. Char-based skip is safe.
        let key = "secret-key-🦀🎉ab";
        let masked = mask_last_four(key);
        assert!(masked.starts_with("****"));
        assert_eq!(masked.chars().count(), 4 + 4);
    }
}
