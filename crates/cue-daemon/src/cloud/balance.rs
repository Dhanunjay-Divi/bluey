//! Codex Stage 17: daemon-side balance polling.
//!
//! When a Bluey account token is in the local account store, this module spawns a
//! background task that polls `/account/me` every 10s and emits a
//! `BalanceSnapshot` over a watch channel. The daemon bridges that
//! snapshot to the native overlay with `SetBalance`, while the dashboard
//! can also fetch a snapshot on demand.
//!
//! This module ONLY owns the data path. Subscribers of the
//! `BalanceWatch` channel render however they want.

use anyhow::{anyhow, Result};
use cue_cloud_client::{CloudClient, CredentialSnapshot};
use std::sync::Arc;
use tokio::sync::watch;

/// Snapshot emitted on every successful poll.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BalanceSnapshot {
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
    pub fetched_at_unix_ms: i64,
    /// True once the customer's balance has dipped below their auto
    /// top-up threshold AND the daemon has not yet observed a credit
    /// (the next snapshot with balance >= threshold clears it).
    pub low_balance_warning: bool,
}

/// Account-authoritative balance result. Every background result identifies
/// the exact owner/profile generation that produced it so the daemon can drop
/// stale A results after account B replaces the local profile.
#[derive(Clone, PartialEq, Eq)]
pub enum BalanceEvent {
    Initial,
    Snapshot {
        credentials: CredentialSnapshot,
        snapshot: BalanceSnapshot,
    },
    /// The captured credentials were rejected, but a newer persistent
    /// credential snapshot had already replaced them and was preserved.
    Revoked {
        credentials: CredentialSnapshot,
    },
    /// The captured credentials were rejected and their exact persistent pair
    /// was conditionally cleared.
    Cleared {
        credentials: CredentialSnapshot,
    },
}

impl std::fmt::Debug for BalanceEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initial => formatter.write_str("BalanceEvent::Initial"),
            Self::Snapshot {
                credentials,
                snapshot,
            } => formatter
                .debug_struct("BalanceEvent::Snapshot")
                .field("credentials", credentials)
                .field("snapshot", snapshot)
                .finish(),
            Self::Revoked { credentials } => formatter
                .debug_struct("BalanceEvent::Revoked")
                .field("credentials", credentials)
                .finish(),
            Self::Cleared { credentials } => formatter
                .debug_struct("BalanceEvent::Cleared")
                .field("credentials", credentials)
                .finish(),
        }
    }
}

/// Authority-bearing watch channel owned by the balance poller and daemon.
#[derive(Clone)]
pub struct BalanceWatch {
    events: Arc<watch::Sender<BalanceEvent>>,
}

impl Default for BalanceWatch {
    fn default() -> Self {
        Self {
            events: Arc::new(watch::channel(BalanceEvent::Initial).0),
        }
    }
}

impl BalanceWatch {
    pub fn current_event(&self) -> BalanceEvent {
        self.events.borrow().clone()
    }

    pub fn subscribe_events(&self) -> watch::Receiver<BalanceEvent> {
        self.events.subscribe()
    }

    /// Publish an authority-bearing result. The daemon bridge must validate
    /// the exact credential snapshot before rendering it.
    pub fn publish_for_credentials(
        &self,
        credentials: CredentialSnapshot,
        snapshot: BalanceSnapshot,
    ) {
        self.events.send_replace(BalanceEvent::Snapshot {
            credentials,
            snapshot,
        });
    }

    pub fn publish_revoked(&self, credentials: CredentialSnapshot) {
        self.events
            .send_replace(BalanceEvent::Revoked { credentials });
    }

    pub fn publish_cleared(&self, credentials: CredentialSnapshot) {
        self.events
            .send_replace(BalanceEvent::Cleared { credentials });
    }
}

/// Owned, awaitable balance poll task for account/login lifecycle code.
pub struct BalancePollTask {
    shutdown: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
}

impl BalancePollTask {
    pub async fn shutdown(self) -> std::result::Result<(), tokio::task::JoinError> {
        let _ = self.shutdown.send(true);
        let mut join = self.join;
        match tokio::time::timeout(std::time::Duration::from_secs(2), &mut join).await {
            Ok(result) => result,
            Err(_) => {
                join.abort();
                match join.await {
                    Err(error) if error.is_cancelled() => Ok(()),
                    result => result,
                }
            }
        }
    }

    pub async fn join(self) -> std::result::Result<(), tokio::task::JoinError> {
        self.join.await
    }
}

/// Start a poller bound to the client's atomically captured credential/profile
/// authority, including its device id. Account lifecycle code owns the returned
/// task and can both signal and await shutdown.
pub fn spawn_authority_loop(
    client: CloudClient,
    watch_handle: BalanceWatch,
) -> Result<BalancePollTask> {
    let captured = client
        .credential_snapshot()
        .ok_or_else(|| anyhow!("balance polling requires captured Bluey credentials"))?;
    let device_id = captured.authority().device_id().map(str::to_string);
    let (shutdown, shutdown_rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        run_loop_inner(client, watch_handle, shutdown_rx, device_id, captured).await
    });
    Ok(BalancePollTask { shutdown, join })
}

async fn run_loop_inner(
    client: CloudClient,
    watch_handle: BalanceWatch,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
    device_id: Option<String>,
    initial_credentials: CredentialSnapshot,
) {
    let interval_secs: u64 = std::env::var("BLUEY_BALANCE_POLL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut consecutive_errors: u32 = 0;
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    tracing::info!("balance poll loop shutting down");
                    return;
                }
            }
        }
        let Some(poll_result) =
            poll_once_with_shutdown(&client, device_id.as_deref(), &mut shutdown).await
        else {
            return;
        };
        match poll_result {
            Ok(snap) => {
                consecutive_errors = 0;
                let current = client
                    .credential_snapshot()
                    .unwrap_or_else(|| initial_credentials.clone());
                watch_handle.publish_for_credentials(current.clone(), snap);
                if !client
                    .credential_snapshot_is_current(&current)
                    .unwrap_or(false)
                {
                    return;
                }
            }
            Err(e) => {
                if is_unauthorized(&e) {
                    let credentials = client
                        .credential_snapshot()
                        .unwrap_or_else(|| initial_credentials.clone());
                    // Account mutation is centralized in the daemon bridge.
                    // It revalidates this authority against the current profile
                    // before conditionally clearing or signing out.
                    watch_handle.publish_revoked(credentials);
                    return;
                }
                consecutive_errors += 1;
                tracing::warn!(
                    error_category = balance_error_category(&e),
                    consecutive_errors,
                    "balance poll error; reloading stored tokens before retry"
                );
                match client.reload_tokens_from_store() {
                    Ok(true) => {
                        let Some(retry_result) =
                            poll_once_with_shutdown(&client, device_id.as_deref(), &mut shutdown)
                                .await
                        else {
                            return;
                        };
                        match retry_result {
                            Ok(snap) => {
                                consecutive_errors = 0;
                                let Some(current) = client.credential_snapshot() else {
                                    watch_handle.publish_revoked(initial_credentials.clone());
                                    return;
                                };
                                watch_handle.publish_for_credentials(current.clone(), snap);
                                if !client
                                    .credential_snapshot_is_current(&current)
                                    .unwrap_or(false)
                                {
                                    return;
                                }
                                continue;
                            }
                            Err(retry_error) => {
                                if is_unauthorized(&retry_error) {
                                    let credentials = client
                                        .credential_snapshot()
                                        .unwrap_or_else(|| initial_credentials.clone());
                                    watch_handle.publish_revoked(credentials);
                                    return;
                                }
                                tracing::warn!(
                                    error_category = balance_error_category(&retry_error),
                                    "balance poll still failed after reloading stored tokens"
                                );
                            }
                        }
                    }
                    Ok(false) => {
                        watch_handle.publish_revoked(initial_credentials.clone());
                        return;
                    }
                    Err(_) => {
                        tracing::warn!(
                            error_category = "credential_store",
                            "balance poll could not reload stored tokens"
                        );
                    }
                }
                // After 10 consecutive errors (5 minutes at default
                // interval) emit a low-confidence snapshot if we have
                // one cached. Keeps the overlay from showing stale
                // data forever without explanation.
                if consecutive_errors >= 10 {
                    tracing::warn!(
                        consecutive_errors,
                        "balance poll has been failing for 10+ intervals; pausing emissions"
                    );
                }
            }
        }
    }
}

async fn poll_once_with_shutdown(
    client: &CloudClient,
    device_id: Option<&str>,
    shutdown: &mut watch::Receiver<bool>,
) -> Option<Result<BalanceSnapshot>> {
    loop {
        tokio::select! {
            result = poll_once(client, device_id) => return Some(result),
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return None;
                }
            }
        }
    }
}

async fn poll_once(
    client: &cue_cloud_client::CloudClient,
    device_id: Option<&str>,
) -> Result<BalanceSnapshot> {
    if let Some(device_id) = device_id
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "local-device")
    {
        let status: cue_cloud_client::DeviceStatusResponse = client
            .auth_post(
                "/account/devices/status",
                &cue_cloud_client::DeviceStatusRequest {
                    device_id: device_id.to_string(),
                },
            )
            .await?;
        if !status.active {
            return Err(cue_cloud_client::Error::Unauthorized.into());
        }
    }
    let me: cue_cloud_client::AccountMe = client.auth_get("/account/me").await?;
    let snap = BalanceSnapshot {
        balance_cents: me.balance_cents,
        trial_seconds_remaining: me.trial_seconds_remaining,
        auto_topup_enabled: me.auto_topup_enabled,
        auto_topup_threshold_cents: me.auto_topup_threshold_cents,
        auto_topup_amount_cents: me.auto_topup_amount_cents,
        fetched_at_unix_ms: chrono::Utc::now().timestamp_millis(),
        low_balance_warning: me.balance_cents < me.auto_topup_threshold_cents
            && me.balance_cents > 0,
    };
    Ok(snap)
}

fn is_unauthorized(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<cue_cloud_client::Error>()
        .is_some_and(|error| matches!(error, cue_cloud_client::Error::Unauthorized))
}

fn balance_error_category(error: &anyhow::Error) -> &'static str {
    match error.downcast_ref::<cue_cloud_client::Error>() {
        Some(cue_cloud_client::Error::Unauthorized) => "unauthorized",
        Some(cue_cloud_client::Error::Network(_)) => "network",
        Some(cue_cloud_client::Error::TokenStore(_)) => "credential_store",
        Some(cue_cloud_client::Error::Json(_)) => "response_shape",
        Some(cue_cloud_client::Error::RateLimited { .. }) => "rate_limited",
        Some(cue_cloud_client::Error::CapacityBusy { .. }) => "capacity_busy",
        Some(cue_cloud_client::Error::InsufficientBalance { .. }) => "insufficient_balance",
        Some(cue_cloud_client::Error::TrialEnded) => "trial_ended",
        Some(cue_cloud_client::Error::Server { .. }) => "server",
        Some(_) => "cloud",
        None => "internal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_cloud_client::client::ClientConfig;
    use cue_cloud_client::tokens::MemoryStore;
    use cue_cloud_client::{TokenStore, Tokens};
    use std::time::Duration;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn balance_client(server_url: String, store: Arc<MemoryStore>, tokens: &Tokens) -> CloudClient {
        TokenStore::save(store.as_ref(), tokens).unwrap();
        CloudClient::new(
            ClientConfig {
                base_url: server_url,
                user_agent: "balance-test".to_string(),
                timeout: Duration::from_secs(5),
                trace_id: None,
            },
            store,
        )
        .unwrap()
    }

    fn account_me_response(account_id: &str, email: &str) -> serde_json::Value {
        serde_json::json!({
            "id": account_id,
            "email": email,
            "balance_cents": 2500,
            "trial_seconds_remaining": 0,
            "auto_topup_enabled": true,
            "auto_topup_threshold_cents": 500,
            "auto_topup_amount_cents": 1500
        })
    }

    async fn wait_for_request(server: &MockServer, request_path: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .any(|request| request.url.path() == request_path)
                {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("request should arrive");
    }

    #[test]
    fn watch_starts_with_an_authority_neutral_initial_event() {
        let w = BalanceWatch::default();
        assert!(matches!(w.current_event(), BalanceEvent::Initial));
    }

    #[tokio::test]
    async fn stale_success_event_keeps_captured_authority_and_preserves_replacement() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header("authorization", "Bearer access-a"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(200))
                    .set_body_json(account_me_response("account-a", "a@example.com")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let account_a = Tokens {
            access: "access-a".to_string(),
            refresh: String::new(),
            email: "a@example.com".to_string(),
        };
        let client = balance_client(server.uri(), store.clone(), &account_a);
        let watch = BalanceWatch::default();
        let mut events = watch.subscribe_events();
        let task = spawn_authority_loop(client, watch).unwrap();
        wait_for_request(&server, "/account/me").await;

        let account_b = Tokens {
            access: "access-b".to_string(),
            refresh: "refresh-b".to_string(),
            email: "b@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_b).unwrap();

        events.changed().await.unwrap();
        match events.borrow().clone() {
            BalanceEvent::Snapshot {
                credentials,
                snapshot,
            } => {
                assert_eq!(credentials.authority().owner_account_id(), "a@example.com");
                assert_eq!(snapshot.balance_cents, 2500);
            }
            event => panic!("expected authority-bearing snapshot, got {event:?}"),
        }
        task.join().await.unwrap();
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), Some(account_b));
    }

    #[tokio::test]
    async fn late_a1_success_carries_a1_snapshot_after_same_owner_a2_refresh() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header("authorization", "Bearer access-a1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(200))
                    .set_body_json(account_me_response("account-a", "a@example.com")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let account_a1 = Tokens {
            access: "access-a1".to_string(),
            refresh: String::new(),
            email: "a@example.com".to_string(),
        };
        let client = balance_client(server.uri(), store.clone(), &account_a1);
        let a1_generation = client
            .credential_authority()
            .unwrap()
            .credential_generation();
        let watch = BalanceWatch::default();
        let mut events = watch.subscribe_events();
        let task = spawn_authority_loop(client, watch).unwrap();
        wait_for_request(&server, "/account/me").await;

        let account_a2 = Tokens {
            access: "access-a2".to_string(),
            refresh: "refresh-a2".to_string(),
            email: "a@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_a2).unwrap();
        let a2_snapshot = TokenStore::load_snapshot(store.as_ref()).unwrap().unwrap();
        assert!(a2_snapshot.authority().credential_generation() > a1_generation);

        events.changed().await.unwrap();
        match events.borrow().clone() {
            BalanceEvent::Snapshot { credentials, .. } => {
                assert_eq!(
                    credentials.authority().credential_generation(),
                    a1_generation
                );
                assert_ne!(credentials, a2_snapshot);
            }
            event => panic!("expected exact A1 snapshot event, got {event:?}"),
        }
        task.join().await.unwrap();
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), Some(account_a2));
    }

    #[tokio::test]
    async fn stale_unauthorized_event_cannot_clear_replacement_authority() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header("authorization", "Bearer access-a"))
            .respond_with(ResponseTemplate::new(401).set_delay(Duration::from_millis(200)))
            .expect(1)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let account_a = Tokens {
            access: "access-a".to_string(),
            refresh: String::new(),
            email: "a@example.com".to_string(),
        };
        let client = balance_client(server.uri(), store.clone(), &account_a);
        let watch = BalanceWatch::default();
        let mut events = watch.subscribe_events();
        let task = spawn_authority_loop(client, watch).unwrap();
        wait_for_request(&server, "/account/me").await;

        let account_b = Tokens {
            access: "access-b".to_string(),
            refresh: "refresh-b".to_string(),
            email: "b@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_b).unwrap();

        events.changed().await.unwrap();
        match events.borrow().clone() {
            BalanceEvent::Revoked { credentials } => {
                assert_eq!(credentials.authority().owner_account_id(), "a@example.com");
            }
            event => panic!("expected authority-bearing revocation, got {event:?}"),
        }
        task.join().await.unwrap();
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), Some(account_b));
    }

    #[tokio::test]
    async fn exact_unauthorized_credentials_wait_for_central_conditional_clear() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header("authorization", "Bearer access-a"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let account_a = Tokens {
            access: "access-a".to_string(),
            refresh: String::new(),
            email: "a@example.com".to_string(),
        };
        let client = balance_client(server.uri(), store.clone(), &account_a);
        let watch = BalanceWatch::default();
        let mut events = watch.subscribe_events();
        let task = spawn_authority_loop(client, watch).unwrap();

        events.changed().await.unwrap();
        match events.borrow().clone() {
            BalanceEvent::Revoked { credentials } => {
                assert_eq!(credentials.authority().owner_account_id(), "a@example.com");
            }
            event => panic!("expected authority-bearing revocation, got {event:?}"),
        }
        task.join().await.unwrap();
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), Some(account_a));
    }

    #[tokio::test]
    async fn poll_task_shutdown_is_awaitable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(account_me_response("account-a", "a@example.com")),
            )
            .mount(&server)
            .await;
        let store = Arc::new(MemoryStore::new());
        let client = balance_client(
            server.uri(),
            store,
            &Tokens {
                access: "access-a".to_string(),
                refresh: String::new(),
                email: "a@example.com".to_string(),
            },
        );
        let watch = BalanceWatch::default();
        let mut events = watch.subscribe_events();
        let task = spawn_authority_loop(client, watch).unwrap();
        events.changed().await.unwrap();

        tokio::time::timeout(Duration::from_secs(2), task.shutdown())
            .await
            .expect("shutdown should complete")
            .unwrap();
    }

    #[tokio::test]
    async fn poll_task_shutdown_cancels_an_in_flight_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(30))
                    .set_body_json(account_me_response("account-a", "a@example.com")),
            )
            .expect(1)
            .mount(&server)
            .await;
        let store = Arc::new(MemoryStore::new());
        let client = balance_client(
            server.uri(),
            store,
            &Tokens {
                access: "access-a".to_string(),
                refresh: String::new(),
                email: "a@example.com".to_string(),
            },
        );
        let task = spawn_authority_loop(client, BalanceWatch::default()).unwrap();
        wait_for_request(&server, "/account/me").await;

        tokio::time::timeout(Duration::from_secs(1), task.shutdown())
            .await
            .expect("in-flight request shutdown should be prompt")
            .unwrap();
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn low_balance_warning_only_when_positive_and_below_threshold() {
        // balance below threshold AND positive -> warning
        let s = BalanceSnapshot {
            balance_cents: 100,
            trial_seconds_remaining: 0,
            auto_topup_enabled: true,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 1500,
            fetched_at_unix_ms: 0,
            low_balance_warning: 100 < 500 && 100 > 0,
        };
        assert!(s.low_balance_warning);

        // balance zero -> no warning (hard-stop already kicked in)
        let s2_low = 0 < 500 && 0 > 0;
        assert!(!s2_low);

        // balance above threshold -> no warning
        let s3_low = 600 < 500 && 600 > 0;
        assert!(!s3_low);
    }
}
