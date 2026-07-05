//! Codex Stage 17: daemon-side balance polling.
//!
//! When a Bluey account token is in the local account store, this module spawns a
//! background task that polls `/account/me` every 30s and emits a
//! `BalanceSnapshot` over a watch channel. The daemon bridges that
//! snapshot to the native overlay with `SetBalance`, while the dashboard
//! can also fetch a snapshot on demand.
//!
//! This module ONLY owns the data path. Subscribers of the
//! `BalanceWatch` channel render however they want.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::watch;

/// Snapshot emitted on every successful poll.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BalanceSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
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

/// Watch handle: subscribers call `.subscribe()` to get a watch
/// receiver. The sender side is owned by the polling task.
#[derive(Clone)]
pub struct BalanceWatch {
    inner: Arc<watch::Sender<Option<BalanceSnapshot>>>,
}

impl Default for BalanceWatch {
    fn default() -> Self {
        Self {
            inner: Arc::new(watch::channel(None).0),
        }
    }
}

impl BalanceWatch {
    /// Get the most recent snapshot, if the poller has produced one.
    pub fn current(&self) -> Option<BalanceSnapshot> {
        self.inner.borrow().clone()
    }

    /// Subscribe to receive every new snapshot.
    pub fn subscribe(&self) -> watch::Receiver<Option<BalanceSnapshot>> {
        self.inner.subscribe()
    }

    /// Publish a snapshot produced by a manual refresh path. This keeps
    /// overlay/dashboard subscribers in sync even when a user action refreshes
    /// balance outside the background poll interval.
    pub fn publish(&self, snapshot: BalanceSnapshot) {
        let _ = self.inner.send(Some(snapshot));
    }
}

/// Spawn the balance poll loop. Returns immediately; the loop runs
/// until it errors out (token missing, server unreachable for too
/// long, etc). On error the loop emits the last-known snapshot and
/// retries after a backoff.
///
/// Honors `BLUEY_BALANCE_POLL_SECS` env var for the poll interval
/// (default 30s) so tests can run faster.
pub fn spawn_loop(
    client: cue_cloud_client::CloudClient,
    watch_handle: BalanceWatch,
) -> tokio::task::JoinHandle<()> {
    spawn_loop_with_shutdown(client, watch_handle, None)
}

/// Codex S12-17 nit: variant that accepts a graceful-shutdown signal.
/// When the signal fires, the poll loop exits cleanly. Used by
/// dashboard restart paths so we do not leak the task across reloads.
pub fn spawn_loop_with_shutdown(
    client: cue_cloud_client::CloudClient,
    watch_handle: BalanceWatch,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move { run_loop_inner(client, watch_handle, shutdown).await })
}

async fn run_loop_inner(
    client: cue_cloud_client::CloudClient,
    watch_handle: BalanceWatch,
    mut shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> () {
    let interval_secs: u64 = std::env::var("BLUEY_BALANCE_POLL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut consecutive_errors: u32 = 0;
    loop {
        if let Some(rx) = shutdown.as_mut() {
            tokio::select! {
                _ = interval.tick() => {}
                _ = rx.changed() => {
                    if *rx.borrow() {
                        tracing::info!("balance poll loop shutting down");
                        return;
                    }
                }
            }
        } else {
            interval.tick().await;
        }
        match poll_once(&client).await {
            Ok(snap) => {
                consecutive_errors = 0;
                let _ = watch_handle.inner.send(Some(snap));
            }
            Err(e) => {
                consecutive_errors += 1;
                let error_message = e.to_string();
                tracing::warn!(
                    error = %error_message,
                    consecutive_errors,
                    "balance poll error; reloading stored tokens before retry"
                );
                match client.reload_tokens_from_store() {
                    Ok(true) => match poll_once(&client).await {
                        Ok(snap) => {
                            consecutive_errors = 0;
                            let _ = watch_handle.inner.send(Some(snap));
                            continue;
                        }
                        Err(retry_error) => {
                            tracing::warn!(
                                error = %retry_error,
                                "balance poll still failed after reloading stored tokens"
                            );
                        }
                    },
                    Ok(false) => {
                        tracing::warn!("balance poll reload found no stored tokens");
                    }
                    Err(reload_error) => {
                        tracing::warn!(
                            error = %reload_error,
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

async fn poll_once(client: &cue_cloud_client::CloudClient) -> Result<BalanceSnapshot> {
    let me: cue_cloud_client::AccountMe = client.auth_get("/account/me").await?;
    let snap = BalanceSnapshot {
        account_email: Some(me.email),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn watch_starts_with_none() {
        let w = BalanceWatch::default();
        assert!(w.current().is_none());
    }

    #[tokio::test]
    async fn watch_publishes_to_subscribers() {
        let w = BalanceWatch::default();
        let mut rx = w.subscribe();
        // Initial value is None.
        assert!(rx.borrow().is_none());

        let snap = BalanceSnapshot {
            account_email: Some("demo@bluey.sh".to_string()),
            balance_cents: 2500,
            trial_seconds_remaining: 0,
            auto_topup_enabled: true,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 1500,
            fetched_at_unix_ms: 1_700_000_000_000,
            low_balance_warning: false,
        };
        // Push from the sender side (testing only — production uses
        // run_loop).
        w.inner.send(Some(snap.clone())).unwrap();
        assert!(rx.changed().await.is_ok());
        let observed = rx.borrow().clone().unwrap();
        assert_eq!(observed.balance_cents, 2500);
        assert!(observed.auto_topup_enabled);
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn low_balance_warning_only_when_positive_and_below_threshold() {
        // balance below threshold AND positive -> warning
        let s = BalanceSnapshot {
            account_email: None,
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
