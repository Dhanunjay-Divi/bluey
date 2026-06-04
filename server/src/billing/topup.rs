//! Auto top-up: when a customer's balance falls below threshold and
//! they've enabled auto top-up + saved a Stripe PaymentMethod, charge
//! the saved card off-session for the configured amount.
//!
//! Codex Stage 10: this is the dealbreaker for paid v0.2. Without it,
//! customers hit the hard-stop at $0 and must manually reload via
//! `bluey usage` or the web dashboard.
//!
//! Trigger pattern (in /router/complete after deduct):
//!   if !on_trial
//!      && account.auto_topup_enabled
//!      && balance_after < account.auto_topup_threshold_cents
//!      && account.stripe_payment_method_id.is_some()
//!      && account.stripe_customer_id.is_some()
//!   {
//!       tokio::spawn(maybe_auto_topup(...))
//!   }
//!
//! The actual charge is fire-and-forget (tokio::spawn) so the customer
//! request returns immediately with the response. The webhook for the
//! resulting payment_intent.succeeded event will credit the balance
//! via the existing /billing/webhook flow.

use anyhow::{anyhow, Context, Result};

use crate::config::Config;
use crate::db::DbPool;

/// In-flight dedupe: stops two concurrent low-balance checks from
/// firing two top-ups for the same account in a 60-second window.
/// Stage 10 nit (deferred): could move this to a SQLite row with a
/// timestamp guard for multi-process safety. Single-process is fine
/// for v0.2 because the server is single-binary.
static AUTO_TOPUP_INFLIGHT: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::OnceLock::new();

const INFLIGHT_DEDUPE_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

fn stripe_api_url(path: &str) -> String {
    let base =
        std::env::var("BLUEY_TEST_STRIPE_URL").unwrap_or_else(|_| "https://api.stripe.com".into());
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

#[allow(clippy::too_many_arguments)]
/// Spawn an async auto top-up if conditions are met. Non-blocking.
/// Returns immediately; the actual Stripe call runs on the executor.
pub fn maybe_spawn(
    pool: DbPool,
    config: std::sync::Arc<Config>,
    account_id: String,
    balance_after_cents: i64,
    auto_topup_enabled: bool,
    auto_topup_threshold_cents: i64,
    stripe_customer_id: Option<String>,
    stripe_payment_method_id: Option<String>,
    auto_topup_amount_cents: i64,
) {
    if !auto_topup_enabled {
        return;
    }
    if balance_after_cents >= auto_topup_threshold_cents {
        return;
    }
    let (Some(customer_id), Some(pm_id)) = (stripe_customer_id, stripe_payment_method_id) else {
        tracing::debug!(
            account_id,
            "auto top-up skipped: no saved Stripe customer/payment_method"
        );
        return;
    };
    if config.stripe_secret_key.is_none() {
        tracing::debug!("auto top-up skipped: STRIPE_SECRET_KEY not configured");
        return;
    }

    tokio::spawn(async move {
        if let Err(e) = run_topup(
            pool,
            config,
            account_id.clone(),
            customer_id,
            pm_id,
            auto_topup_amount_cents,
        )
        .await
        {
            tracing::warn!(account_id, error = %e, "auto top-up failed");
        }
    });
}

async fn run_topup(
    _pool: DbPool,
    config: std::sync::Arc<Config>,
    account_id: String,
    stripe_customer_id: String,
    stripe_payment_method_id: String,
    amount_cents: i64,
) -> Result<()> {
    // In-flight dedupe: do NOT fire if we already fired for this account
    // within the last 60s. This protects against the case where a burst
    // of cues each see the same low balance before the previous top-up
    // webhook has credited.
    {
        let mu = AUTO_TOPUP_INFLIGHT.get_or_init(Default::default);
        let mut guard = mu.lock().await;
        let now = std::time::Instant::now();
        // Sweep stale entries.
        guard.retain(|_, ts| now.duration_since(*ts) < INFLIGHT_DEDUPE_WINDOW);
        if guard.contains_key(&account_id) {
            tracing::debug!(account_id, "auto top-up skipped: already in-flight");
            return Ok(());
        }
        guard.insert(account_id.clone(), now);
    }

    let stripe_key = config
        .stripe_secret_key
        .as_ref()
        .ok_or_else(|| anyhow!("STRIPE_SECRET_KEY not configured"))?;

    // Idempotency: Stripe's Idempotency-Key header keys ON the request.
    // Use a deterministic key per (account, hour) so a duplicate fire
    // from the same process within the same hour will return Stripe's
    // cached response instead of charging twice.
    let idempotency_key = format!(
        "bluey-topup-{account_id}-{}",
        chrono::Utc::now().format("%Y%m%d%H")
    );

    let form = [
        ("amount", amount_cents.to_string()),
        ("currency", "usd".to_string()),
        ("customer", stripe_customer_id),
        ("payment_method", stripe_payment_method_id),
        ("off_session", "true".to_string()),
        ("confirm", "true".to_string()),
        ("metadata[bluey_account_id]", account_id.clone()),
        ("metadata[bluey_amount_cents]", amount_cents.to_string()),
        ("metadata[bluey_kind]", "auto_topup".to_string()),
    ];

    let resp = reqwest::Client::new()
        .post(stripe_api_url("/v1/payment_intents"))
        .basic_auth(stripe_key, Some(""))
        .header("Idempotency-Key", &idempotency_key)
        .form(&form)
        .send()
        .await
        .context("stripe payment_intents.create http")?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .unwrap_or_else(|_| "<failed to read body>".to_string());

    if !status.is_success() {
        return Err(anyhow!("stripe payment_intents.create {status}: {body}"));
    }

    tracing::info!(
        account_id,
        amount_cents,
        idempotency_key,
        "auto top-up charge initiated; webhook will credit balance"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-topup-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn test_config() -> std::sync::Arc<Config> {
        std::sync::Arc::new(Config {
            port: 0,
            db_path: std::path::PathBuf::from(":memory:"),
            jwt_secret: "test_secret_at_least_32_chars_long_xx".to_string(),
            public_url: "http://localhost".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            upstream: crate::config::UpstreamKeys::default(),
            upstream_spend_guard: None,
            smtp: None,
            admin_emails: vec![],
        })
    }

    #[tokio::test]
    async fn skip_when_disabled() {
        let pool = temp_pool();
        // Should silently noop because auto_topup_enabled = false.
        maybe_spawn(
            pool,
            test_config(),
            "acc1".into(),
            0,
            false,
            500,
            Some("cus_1".into()),
            Some("pm_1".into()),
            3000,
        );
        // Test passes if we get here without panicking. Fire-and-forget
        // task, if any, would not have run because of the early return.
    }

    #[tokio::test]
    async fn skip_when_balance_above_threshold() {
        let pool = temp_pool();
        maybe_spawn(
            pool,
            test_config(),
            "acc2".into(),
            1000,
            true,
            500,
            Some("cus_1".into()),
            Some("pm_1".into()),
            3000,
        );
    }

    #[tokio::test]
    async fn skip_when_no_payment_method() {
        let pool = temp_pool();
        maybe_spawn(
            pool,
            test_config(),
            "acc3".into(),
            100,
            true,
            500,
            Some("cus_1".into()),
            None, // no PM
            3000,
        );
    }
}
