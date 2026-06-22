//! Processor-backed auto reload.
//!
//! New accounts default to manual reload. When a user explicitly enables
//! Auto Reload, this module charges the active processor only if that
//! account has a saved off-session payment method. Spendable balance is
//! credited only from processor-confirmed payment ids.
//!
//! Codex Stage 10: this is the dealbreaker for paid v0.2. Without it,
//! customers hit the hard-stop at $0 and must manually reload via
//! `bluey usage` or the web dashboard.
//!
//! Trigger pattern (in /router/complete after deduct):
//!   if !on_trial
//!      && account.auto_topup_enabled
//!      && balance_after < account.auto_topup_threshold_cents
//!      && account has a saved payment method for active billing provider
//!   {
//!       tokio::spawn(maybe_auto_topup(...))
//!   }
//!
//! The actual charge is fire-and-forget (tokio::spawn) so the customer
//! request returns immediately with the response. Stripe credits via
//! webhook; Square credits immediately only after a COMPLETED payment
//! response and then treats the later webhook as an idempotent no-op.

use anyhow::{anyhow, Context, Result};

use crate::config::{BillingProvider, Config};
use crate::db::{accounts::Account, balance, DbPool};

/// In-flight dedupe: stops two concurrent low-balance checks from
/// firing two top-ups for the same account in a 60-second window.
/// Stage 10 nit (deferred): could move this to a SQLite row with a
/// timestamp guard for multi-process safety. Single-process is fine
/// for v0.2 because the server is single-binary.
static AUTO_TOPUP_INFLIGHT: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::OnceLock::new();

const INFLIGHT_DEDUPE_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
const SQUARE_API_VERSION: &str = "2025-04-16";

fn stripe_api_url(path: &str) -> String {
    let base =
        std::env::var("BLUEY_TEST_STRIPE_URL").unwrap_or_else(|_| "https://api.stripe.com".into());
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn square_api_url(config: &crate::config::SquareConfig, path: &str) -> String {
    let base = std::env::var("BLUEY_TEST_SQUARE_URL")
        .unwrap_or_else(|_| config.environment.api_base_url().to_string());
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

fn square_reload_reference_id(account_id: &str) -> String {
    let compact = account_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect::<String>();
    format!("br_{compact}")
}

fn square_topup_idempotency_key(account_id: &str) -> String {
    format!(
        "bst-{}-{}",
        cue_core::account_id_hash_prefix(account_id),
        chrono::Utc::now().format("%Y%m%d%H")
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
    square_customer_id: Option<String>,
    square_card_id: Option<String>,
    auto_topup_amount_cents: i64,
) {
    if !auto_topup_enabled {
        return;
    }
    if balance_after_cents >= auto_topup_threshold_cents {
        return;
    }
    match Account::fetch_by_id(&pool, &account_id) {
        Ok(Some(account)) if account.billing_restricted => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                "auto reload skipped: account billing is restricted"
            );
            return;
        }
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                "auto reload skipped: account not found"
            );
            return;
        }
        Err(e) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                error = %e,
                "auto reload skipped: account lookup failed"
            );
            return;
        }
    }

    match config.billing_provider() {
        BillingProvider::Stripe => {
            let (Some(customer_id), Some(pm_id)) = (stripe_customer_id, stripe_payment_method_id)
            else {
                tracing::debug!(
                    account_id,
                    "auto reload skipped: no saved Stripe customer/payment_method"
                );
                return;
            };
            if config.stripe_secret_key.is_none() {
                tracing::debug!("auto reload skipped: STRIPE_SECRET_KEY not configured");
                return;
            }

            tokio::spawn(async move {
                if let Err(e) = run_stripe_topup(
                    pool,
                    config,
                    account_id.clone(),
                    customer_id,
                    pm_id,
                    auto_topup_amount_cents,
                )
                .await
                {
                    tracing::warn!(account_id, error = %e, "auto reload failed");
                }
            });
        }
        BillingProvider::Square => {
            let (Some(customer_id), Some(card_id)) = (square_customer_id, square_card_id) else {
                tracing::debug!(account_id, "auto reload skipped: no saved Square card");
                return;
            };
            let square = config.square_config();
            if square.access_token.is_none() || square.location_id.is_none() {
                tracing::debug!("auto reload skipped: Square billing not configured");
                return;
            }

            tokio::spawn(async move {
                if let Err(e) = run_square_topup(
                    pool,
                    config,
                    account_id.clone(),
                    customer_id,
                    card_id,
                    auto_topup_amount_cents,
                )
                .await
                {
                    tracing::warn!(account_id, error = %e, "auto reload failed");
                }
            });
        }
    }
}

async fn reserve_inflight(account_id: &str) -> bool {
    let mu = AUTO_TOPUP_INFLIGHT.get_or_init(Default::default);
    let mut guard = mu.lock().await;
    let now = std::time::Instant::now();
    guard.retain(|_, ts| now.duration_since(*ts) < INFLIGHT_DEDUPE_WINDOW);
    if guard.contains_key(account_id) {
        tracing::debug!(account_id, "auto reload skipped: already in-flight");
        return false;
    }
    guard.insert(account_id.to_string(), now);
    true
}

async fn run_stripe_topup(
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
    if !reserve_inflight(&account_id).await {
        return Ok(());
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
    let _body = resp
        .text()
        .await
        .unwrap_or_else(|_| "<failed to read body>".to_string());

    if !status.is_success() {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            amount_cents,
            stripe_status = %status,
            "stripe auto reload payment failed"
        );
        return Err(anyhow!("stripe payment_intents.create returned {status}"));
    }

    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
        amount_cents,
        idempotency_key,
        "auto reload charge initiated; webhook will credit balance"
    );
    Ok(())
}

async fn run_square_topup(
    pool: DbPool,
    config: std::sync::Arc<Config>,
    account_id: String,
    square_customer_id: String,
    square_card_id: String,
    amount_cents: i64,
) -> Result<()> {
    if !reserve_inflight(&account_id).await {
        return Ok(());
    }

    let square = config.square_config();
    let access_token = square
        .access_token
        .as_ref()
        .ok_or_else(|| anyhow!("SQUARE_ACCESS_TOKEN not configured"))?;
    let location_id = square
        .location_id
        .as_ref()
        .ok_or_else(|| anyhow!("SQUARE_LOCATION_ID not configured"))?;
    let idempotency_key = square_topup_idempotency_key(&account_id);
    let body = serde_json::json!({
        "idempotency_key": idempotency_key,
        "source_id": square_card_id,
        "amount_money": {
            "amount": amount_cents,
            "currency": "USD"
        },
        "customer_id": square_customer_id,
        "location_id": location_id,
        "autocomplete": true,
        "reference_id": square_reload_reference_id(&account_id),
        "note": "Bluey auto reload"
    });
    let expected_reference_id = square_reload_reference_id(&account_id);

    let resp = reqwest::Client::new()
        .post(square_api_url(&square, "/v2/payments"))
        .bearer_auth(access_token)
        .header("Square-Version", SQUARE_API_VERSION)
        .json(&body)
        .send()
        .await
        .context("square payments.create http")?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            amount_cents,
            square_status = %status,
            "Square auto reload payment failed"
        );
        return Err(anyhow!("square payments.create returned {status}"));
    }

    let payment_status = body
        .pointer("/payment/status")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");
    let payment = body
        .pointer("/payment")
        .ok_or_else(|| anyhow!("square payment response missing payment"))?;
    validate_square_payment_response(
        payment,
        &account_id,
        &expected_reference_id,
        &square_customer_id,
        amount_cents,
    )?;

    let payment_id = payment
        .pointer("/id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("square payment response missing payment.id"))?;

    if payment_status == "COMPLETED" {
        let credited = balance::credit_processor_payment(
            &pool,
            &account_id,
            amount_cents,
            "square",
            payment_id,
        )
        .context("credit Square auto reload")?;
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            amount_cents,
            square_payment_id = %payment_id,
            credited,
            "Square auto reload completed"
        );
    } else {
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            amount_cents,
            square_payment_id = %payment_id,
            square_payment_status = %payment_status,
            "Square auto reload payment created; waiting for webhook completion"
        );
    }

    Ok(())
}

fn validate_square_payment_response(
    payment: &serde_json::Value,
    account_id: &str,
    expected_reference_id: &str,
    expected_customer_id: &str,
    expected_amount_cents: i64,
) -> Result<()> {
    let observed_amount = payment
        .pointer("/amount_money/amount")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow!("square payment response missing amount_money.amount"))?;
    let observed_currency = payment
        .pointer("/amount_money/currency")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("square payment response missing amount_money.currency"))?;
    if observed_currency != "USD" {
        return Err(anyhow!(
            "Square auto reload currency mismatch: expected USD, got {observed_currency}"
        ));
    }
    if observed_amount != expected_amount_cents {
        return Err(anyhow!(
            "Square auto reload amount mismatch: expected {expected_amount_cents} cents, got {observed_amount} cents"
        ));
    }

    let observed_reference_id = payment
        .pointer("/reference_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("square payment response missing reference_id"))?;
    if observed_reference_id != expected_reference_id {
        return Err(anyhow!(
            "Square auto reload reference mismatch for account {}",
            cue_core::account_id_hash_prefix(account_id)
        ));
    }

    if let Some(observed_customer_id) = payment.pointer("/customer_id").and_then(|v| v.as_str()) {
        if observed_customer_id != expected_customer_id {
            return Err(anyhow!(
                "Square auto reload customer mismatch for account {}",
                cue_core::account_id_hash_prefix(account_id)
            ));
        }
    }

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
            db_backend: crate::config::ServerDbBackend::Sqlite,
            database_url: None,
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
            None,
            None,
            1500,
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
            None,
            None,
            1500,
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
            None,
            None,
            1500,
        );
    }

    #[test]
    fn square_topup_idempotency_key_stays_within_square_limit() {
        let key = square_topup_idempotency_key("833e66ac-0652-43c7-a55e-8d51d9ccc982");
        assert!(key.len() <= 45, "{key}");
        assert!(key.starts_with("bst-"));
    }
}
