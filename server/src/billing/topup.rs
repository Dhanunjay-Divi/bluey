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
//! request returns immediately with the response. Stripe credits from a
//! validated success response, webhook, or reconciliation; all three share
//! one durable exactly-once transition. Square credits immediately only after
//! a COMPLETED payment response and treats the later webhook as a no-op.

use anyhow::{anyhow, bail, Context, Result};

use crate::config::{BillingProvider, Config};
use crate::db::{
    accounts::Account,
    balance,
    stripe_auto_reload::{self, CreditDisposition, StripeAutoReloadAttempt},
    DbPool,
};

/// Square in-flight dedupe. Stripe uses a durable cross-process reservation.
static AUTO_TOPUP_INFLIGHT: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::OnceLock::new();

const INFLIGHT_DEDUPE_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
const SQUARE_API_VERSION: &str = "2025-04-16";
const STRIPE_AUTO_RELOAD_ENABLED_ENV: &str = "BLUEY_STRIPE_AUTO_RELOAD_ENABLED";
const STRIPE_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

fn stripe_auto_reload_opted_in() -> bool {
    std::env::var(STRIPE_AUTO_RELOAD_ENABLED_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn stripe_auto_reload_readiness(
    config: &Config,
    opted_in: bool,
) -> std::result::Result<(), &'static str> {
    if !opted_in {
        return Err("BLUEY_STRIPE_AUTO_RELOAD_ENABLED is not true");
    }
    if config
        .stripe_secret_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err("STRIPE_SECRET_KEY is missing");
    }
    if config
        .stripe_webhook_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err("STRIPE_WEBHOOK_SECRET is missing");
    }
    Ok(())
}

pub(crate) fn stripe_auto_reload_configuration_error(config: &Config) -> Option<&'static str> {
    stripe_auto_reload_readiness(config, stripe_auto_reload_opted_in()).err()
}

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
        Ok(Some(account))
            if crate::billing::policy::is_internal_or_test_billing_account(&account) =>
        {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                "auto reload skipped: internal/test billing account"
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
            let (Some(_customer_id), Some(_pm_id)) = (stripe_customer_id, stripe_payment_method_id)
            else {
                tracing::debug!(
                    account_id,
                    "auto reload skipped: no saved Stripe customer/payment_method"
                );
                return;
            };
            if let Err(reason) =
                stripe_auto_reload_readiness(&config, stripe_auto_reload_opted_in())
            {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                    reason,
                    "Stripe Auto Reload disabled; no payment will be attempted"
                );
                return;
            }

            tokio::spawn(async move {
                if let Err(e) = run_stripe_topup(pool, config, account_id.clone()).await {
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
    pool: DbPool,
    config: std::sync::Arc<Config>,
    account_id: String,
) -> Result<()> {
    stripe_auto_reload_readiness(&config, stripe_auto_reload_opted_in())
        .map_err(|reason| anyhow!("Stripe Auto Reload disabled: {reason}"))?;

    let Some(mut attempt) = stripe_auto_reload::reserve_if_eligible(&pool, &account_id)? else {
        return Ok(());
    };

    let stripe_key = config
        .stripe_secret_key
        .as_ref()
        .ok_or_else(|| anyhow!("STRIPE_SECRET_KEY not configured"))?;
    let client = reqwest::Client::builder()
        .timeout(STRIPE_HTTP_TIMEOUT)
        .build()
        .context("build Stripe Auto Reload client")?;

    if attempt.stripe_payment_intent_id.is_some() {
        match reconcile_stripe_attempt(&pool, &client, stripe_key, &attempt).await? {
            StripeIntentOutcome::RequiresConfirmation => {
                attempt = stripe_auto_reload::find_by_id(&pool, &attempt.id)?
                    .ok_or_else(|| anyhow!("Stripe Auto Reload attempt disappeared"))?;
            }
            StripeIntentOutcome::Terminal | StripeIntentOutcome::Pending => return Ok(()),
        }
    } else {
        let intent = create_unconfirmed_stripe_intent(&pool, &client, stripe_key, &attempt).await?;
        let payment_intent_id = required_stripe_id(&intent, "id")?;
        attempt =
            stripe_auto_reload::attach_payment_intent(&pool, &attempt.id, &payment_intent_id)?;
        let outcome = match apply_stripe_intent(&pool, &attempt, &intent, None) {
            Ok(outcome) => outcome,
            Err(error) => {
                stripe_auto_reload::mark_failed_and_disable(
                    &pool,
                    &attempt.id,
                    "processor_response_mismatch",
                    None,
                )?;
                let _ = cancel_unconfirmed_stripe_intent(&client, stripe_key, &attempt).await;
                return Err(error);
            }
        };
        match outcome {
            StripeIntentOutcome::RequiresConfirmation => {}
            StripeIntentOutcome::Terminal | StripeIntentOutcome::Pending => return Ok(()),
        }
    }

    if !stripe_auto_reload::confirmation_allowed(&pool, &attempt.id)? {
        let _ = cancel_unconfirmed_stripe_intent(&client, stripe_key, &attempt).await;
        stripe_auto_reload::mark_abandoned(&pool, &attempt.id, "eligibility_changed")?;
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            attempt_id = %attempt.id,
            "Stripe Auto Reload canceled before confirmation because eligibility changed"
        );
        return Ok(());
    }

    confirm_stripe_intent(&pool, &client, stripe_key, &attempt).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StripeIntentOutcome {
    RequiresConfirmation,
    Pending,
    Terminal,
}

async fn create_unconfirmed_stripe_intent(
    pool: &DbPool,
    client: &reqwest::Client,
    stripe_key: &str,
    attempt: &StripeAutoReloadAttempt,
) -> Result<serde_json::Value> {
    let form = [
        ("amount", attempt.amount_cents.to_string()),
        ("currency", attempt.currency.clone()),
        ("customer", attempt.stripe_customer_id.clone()),
        ("payment_method", attempt.stripe_payment_method_id.clone()),
        ("payment_method_types[]", "card".to_string()),
        ("confirmation_method", "automatic".to_string()),
        ("description", "Bluey Auto Reload".to_string()),
        ("metadata[bluey_account_id]", attempt.account_id.clone()),
        (
            "metadata[bluey_amount_cents]",
            attempt.amount_cents.to_string(),
        ),
        ("metadata[bluey_kind]", "auto_topup".to_string()),
        ("metadata[bluey_auto_reload_attempt_id]", attempt.id.clone()),
        ("metadata[bluey_auto_reload_version]", "v1".to_string()),
    ];
    let response = match client
        .post(stripe_api_url("/v1/payment_intents"))
        .basic_auth(stripe_key, Some(""))
        .header("Idempotency-Key", &attempt.create_idempotency_key)
        .form(&form)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                "create_transport_error",
            )?;
            return Err(error).context("Stripe PaymentIntent create transport");
        }
    };
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let error_code = stripe_error_code(&body, "payment_intent_create_failed");
        if status.is_server_error() || status.as_u16() == 429 {
            stripe_auto_reload::mark_reconciliation_required(pool, &attempt.id, &error_code)?;
        } else {
            stripe_auto_reload::mark_failed_and_disable(pool, &attempt.id, &error_code, None)?;
        }
        return Err(anyhow!("Stripe PaymentIntent create returned {status}"));
    }

    Ok(body)
}

async fn confirm_stripe_intent(
    pool: &DbPool,
    client: &reqwest::Client,
    stripe_key: &str,
    attempt: &StripeAutoReloadAttempt,
) -> Result<()> {
    let payment_intent_id = attempt
        .stripe_payment_intent_id
        .as_deref()
        .ok_or_else(|| anyhow!("Stripe Auto Reload attempt has no PaymentIntent"))?;
    let path = format!("/v1/payment_intents/{payment_intent_id}/confirm");
    let response = match client
        .post(stripe_api_url(&path))
        .basic_auth(stripe_key, Some(""))
        .header("Idempotency-Key", &attempt.confirm_idempotency_key)
        .form(&[
            ("off_session", "true"),
            ("error_on_requires_action", "true"),
        ])
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                "confirm_transport_error",
            )?;
            reconcile_after_uncertain_confirmation(pool, client, stripe_key, attempt).await?;
            return Err(error).context("Stripe PaymentIntent confirm transport");
        }
    };
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        if let Some(intent) = body.pointer("/error/payment_intent") {
            if validate_stripe_intent_identity(attempt, intent).is_ok() {
                let outcome = apply_stripe_intent(pool, attempt, intent, None)?;
                if outcome != StripeIntentOutcome::RequiresConfirmation {
                    return Ok(());
                }
            }
        }

        let error_code = stripe_error_code(&body, "payment_intent_confirm_failed");
        if status.is_server_error() || status.as_u16() == 429 {
            stripe_auto_reload::mark_reconciliation_required(pool, &attempt.id, &error_code)?;
            reconcile_after_uncertain_confirmation(pool, client, stripe_key, attempt).await?;
        } else {
            stripe_auto_reload::mark_failed_and_disable(pool, &attempt.id, &error_code, None)?;
        }
        return Err(anyhow!("Stripe PaymentIntent confirm returned {status}"));
    }

    match apply_stripe_intent(pool, attempt, &body, None)? {
        StripeIntentOutcome::RequiresConfirmation => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                "confirm_returned_requires_confirmation",
            )?;
            Err(anyhow!(
                "Stripe confirm response remained requires_confirmation"
            ))
        }
        StripeIntentOutcome::Pending | StripeIntentOutcome::Terminal => Ok(()),
    }
}

async fn reconcile_after_uncertain_confirmation(
    pool: &DbPool,
    client: &reqwest::Client,
    stripe_key: &str,
    attempt: &StripeAutoReloadAttempt,
) -> Result<()> {
    match reconcile_stripe_attempt(pool, client, stripe_key, attempt).await {
        Ok(StripeIntentOutcome::RequiresConfirmation) => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                "confirmation_outcome_unknown",
            )?;
        }
        Ok(StripeIntentOutcome::Pending | StripeIntentOutcome::Terminal) => {}
        Err(error) => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                "reconciliation_transport_error",
            )?;
            return Err(error);
        }
    }
    Ok(())
}

async fn reconcile_stripe_attempt(
    pool: &DbPool,
    client: &reqwest::Client,
    stripe_key: &str,
    attempt: &StripeAutoReloadAttempt,
) -> Result<StripeIntentOutcome> {
    let payment_intent_id = attempt
        .stripe_payment_intent_id
        .as_deref()
        .ok_or_else(|| anyhow!("Stripe Auto Reload attempt has no PaymentIntent"))?;
    let path = format!("/v1/payment_intents/{payment_intent_id}");
    let response = client
        .get(stripe_api_url(&path))
        .basic_auth(stripe_key, Some(""))
        .query(&[("expand[]", "latest_charge")])
        .send()
        .await
        .context("Stripe PaymentIntent reconciliation transport")?;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        stripe_auto_reload::mark_reconciliation_required(
            pool,
            &attempt.id,
            &stripe_error_code(&body, "payment_intent_retrieve_failed"),
        )?;
        return Err(anyhow!(
            "Stripe PaymentIntent reconciliation returned {status}"
        ));
    }
    apply_stripe_intent(pool, attempt, &body, None)
}

async fn cancel_unconfirmed_stripe_intent(
    client: &reqwest::Client,
    stripe_key: &str,
    attempt: &StripeAutoReloadAttempt,
) -> Result<()> {
    let Some(payment_intent_id) = attempt.stripe_payment_intent_id.as_deref() else {
        return Ok(());
    };
    let path = format!("/v1/payment_intents/{payment_intent_id}/cancel");
    let response = client
        .post(stripe_api_url(&path))
        .basic_auth(stripe_key, Some(""))
        .header("Idempotency-Key", format!("bluey-ar-cancel-{}", attempt.id))
        .send()
        .await
        .context("cancel unconfirmed Stripe PaymentIntent")?;
    if !response.status().is_success() {
        bail!("Stripe PaymentIntent cancel returned {}", response.status());
    }
    Ok(())
}

fn apply_stripe_intent(
    pool: &DbPool,
    attempt: &StripeAutoReloadAttempt,
    intent: &serde_json::Value,
    event_id: Option<&str>,
) -> Result<StripeIntentOutcome> {
    validate_stripe_intent_identity(attempt, intent)?;
    let payment_intent_id = required_stripe_id(intent, "id")?;
    let status = intent
        .get("status")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("Stripe PaymentIntent is missing status"))?;
    let charge_id = stripe_intent_charge_id(intent);

    if stripe_intent_is_reversed(intent) {
        let reconciliation_event = event_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("reconcile:{}", attempt.id));
        stripe_auto_reload::reverse_and_restrict(
            pool,
            &attempt.id,
            "stripe_reconciliation_reversal",
            &reconciliation_event,
        )?;
        return Ok(StripeIntentOutcome::Terminal);
    }

    match status {
        "requires_confirmation" => Ok(StripeIntentOutcome::RequiresConfirmation),
        "processing" => {
            stripe_auto_reload::mark_processing(pool, &attempt.id, charge_id.as_deref(), event_id)?;
            Ok(StripeIntentOutcome::Pending)
        }
        "succeeded" => {
            let amount_received = intent
                .get("amount_received")
                .and_then(|value| value.as_i64())
                .ok_or_else(|| anyhow!("succeeded Stripe PaymentIntent has no amount_received"))?;
            if amount_received != attempt.amount_cents {
                bail!(
                    "Stripe Auto Reload amount_received mismatch: expected {}, got {}",
                    attempt.amount_cents,
                    amount_received
                );
            }
            let disposition = stripe_auto_reload::credit_succeeded(
                pool,
                &attempt.id,
                &payment_intent_id,
                charge_id.as_deref(),
                event_id,
            )?;
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&attempt.account_id),
                attempt_id = %attempt.id,
                stripe_payment_intent_id = %payment_intent_id,
                amount_cents = attempt.amount_cents,
                credited = matches!(disposition, CreditDisposition::Credited),
                suppressed_after_reversal = matches!(
                    disposition,
                    CreditDisposition::SuppressedAfterReversal
                ),
                "Stripe Auto Reload reached terminal success"
            );
            Ok(StripeIntentOutcome::Terminal)
        }
        "requires_payment_method" | "requires_action" | "canceled" => {
            let code = intent
                .pointer("/last_payment_error/code")
                .and_then(|value| value.as_str())
                .unwrap_or(status);
            stripe_auto_reload::mark_failed_and_disable(pool, &attempt.id, code, event_id)?;
            Ok(StripeIntentOutcome::Terminal)
        }
        "requires_capture" => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                "unexpected_requires_capture",
            )?;
            Ok(StripeIntentOutcome::Pending)
        }
        other => {
            stripe_auto_reload::mark_reconciliation_required(
                pool,
                &attempt.id,
                &format!("unexpected_status_{other}"),
            )?;
            Ok(StripeIntentOutcome::Pending)
        }
    }
}

fn validate_stripe_intent_identity(
    attempt: &StripeAutoReloadAttempt,
    intent: &serde_json::Value,
) -> Result<()> {
    let payment_intent_id = required_stripe_id(intent, "id")?;
    if let Some(expected) = attempt.stripe_payment_intent_id.as_deref() {
        if expected != payment_intent_id {
            bail!("Stripe PaymentIntent id mismatch for durable Auto Reload attempt");
        }
    }
    let amount = intent
        .get("amount")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| anyhow!("Stripe PaymentIntent is missing amount"))?;
    if amount != attempt.amount_cents {
        bail!(
            "Stripe Auto Reload amount mismatch: expected {}, got {}",
            attempt.amount_cents,
            amount
        );
    }
    let currency = intent
        .get("currency")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("Stripe PaymentIntent is missing currency"))?;
    if currency != attempt.currency {
        bail!("Stripe Auto Reload currency mismatch");
    }
    let customer = intent
        .get("customer")
        .and_then(stripe_object_id)
        .ok_or_else(|| anyhow!("Stripe PaymentIntent is missing customer"))?;
    if customer != attempt.stripe_customer_id {
        bail!("Stripe Auto Reload customer mismatch");
    }
    if let Some(payment_method) = intent.get("payment_method").and_then(stripe_object_id) {
        if payment_method != attempt.stripe_payment_method_id {
            bail!("Stripe Auto Reload payment method mismatch");
        }
    }

    let metadata = intent
        .get("metadata")
        .ok_or_else(|| anyhow!("Stripe Auto Reload PaymentIntent has no metadata"))?;
    let metadata_value = |key: &str| metadata.get(key).and_then(|value| value.as_str());
    if metadata_value("bluey_kind") != Some("auto_topup")
        || metadata_value("bluey_auto_reload_version") != Some("v1")
        || metadata_value("bluey_auto_reload_attempt_id") != Some(attempt.id.as_str())
        || metadata_value("bluey_account_id") != Some(attempt.account_id.as_str())
        || metadata_value("bluey_amount_cents").and_then(|value| value.parse::<i64>().ok())
            != Some(attempt.amount_cents)
    {
        bail!("Stripe Auto Reload metadata does not match durable attempt");
    }
    Ok(())
}

fn required_stripe_id(value: &serde_json::Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(stripe_object_id)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Stripe object is missing {key}"))
}

fn stripe_object_id(value: &serde_json::Value) -> Option<String> {
    value.as_str().map(str::to_string).or_else(|| {
        value
            .get("id")
            .and_then(|id| id.as_str())
            .map(str::to_string)
    })
}

pub(crate) fn stripe_intent_charge_id(intent: &serde_json::Value) -> Option<String> {
    intent
        .get("latest_charge")
        .and_then(stripe_object_id)
        .or_else(|| {
            intent
                .pointer("/charges/data/0/id")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
}

fn stripe_intent_is_reversed(intent: &serde_json::Value) -> bool {
    let Some(charge) = intent
        .get("latest_charge")
        .filter(|value| value.is_object())
    else {
        return false;
    };
    charge
        .get("refunded")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || charge
            .get("disputed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        || charge
            .get("amount_refunded")
            .and_then(|value| value.as_i64())
            .unwrap_or(0)
            > 0
}

fn stripe_error_code(body: &serde_json::Value, fallback: &str) -> String {
    body.pointer("/error/code")
        .or_else(|| body.pointer("/error/decline_code"))
        .and_then(|value| value.as_str())
        .unwrap_or(fallback)
        .to_string()
}

/// Apply a signed Stripe PaymentIntent webhook to a durable Auto Reload.
/// Returns false for unrelated PaymentIntents such as manual Checkout charges.
pub(crate) fn handle_stripe_payment_intent_event(
    pool: &DbPool,
    event_type: &str,
    event_id: &str,
    event: &serde_json::Value,
) -> Result<bool> {
    let intent = event
        .pointer("/data/object")
        .ok_or_else(|| anyhow!("Stripe PaymentIntent event has no data.object"))?;
    let payment_intent_id = required_stripe_id(intent, "id")?;
    let metadata_kind = intent
        .pointer("/metadata/bluey_kind")
        .and_then(|value| value.as_str());
    let attempt = stripe_auto_reload::find_by_payment_intent(pool, &payment_intent_id)?;
    let Some(attempt) = attempt else {
        if metadata_kind == Some("auto_topup") {
            bail!(
                "Stripe Auto Reload PaymentIntent has no durable attempt; reconciliation required"
            );
        }
        return Ok(false);
    };
    if metadata_kind != Some("auto_topup") {
        bail!("durable Stripe Auto Reload PaymentIntent lost its ownership metadata");
    }

    let observed_status = intent
        .get("status")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("Stripe PaymentIntent event has no status"))?;
    match event_type {
        "payment_intent.succeeded" if observed_status != "succeeded" => {
            bail!("payment_intent.succeeded carried non-succeeded object")
        }
        "payment_intent.payment_failed"
            if !matches!(
                observed_status,
                "requires_payment_method" | "requires_action"
            ) =>
        {
            bail!("payment_intent.payment_failed carried unexpected status")
        }
        "payment_intent.requires_action" if observed_status != "requires_action" => {
            bail!("payment_intent.requires_action carried unexpected status")
        }
        "payment_intent.canceled" if observed_status != "canceled" => {
            bail!("payment_intent.canceled carried non-canceled object")
        }
        "payment_intent.processing" if observed_status != "processing" => {
            bail!("payment_intent.processing carried non-processing object")
        }
        _ => {}
    }
    apply_stripe_intent(pool, &attempt, intent, Some(event_id))?;
    Ok(true)
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
            trial_abuse: crate::config::TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: None,
            log_storage: None,
        })
    }

    fn test_config_with_stripe() -> std::sync::Arc<Config> {
        let mut config = (*test_config()).clone();
        config.stripe_secret_key = Some("sk_test".to_string());
        config.stripe_webhook_secret = Some("whsec_test".to_string());
        std::sync::Arc::new(config)
    }

    fn attached_stripe_attempt(
        pool: &DbPool,
        email: &str,
        payment_intent_id: &str,
    ) -> StripeAutoReloadAttempt {
        let account = Account::create(pool, email, "password-hash").unwrap();
        Account::mark_email_verified(pool, &account.id).unwrap();
        Account::save_stripe_checkout_refs(
            pool,
            &account.id,
            Some("cus_auto_reload"),
            Some("pm_auto_reload"),
        )
        .unwrap();
        Account::update_auto_topup_settings(pool, &account.id, true, 500, 1500).unwrap();
        let attempt = stripe_auto_reload::reserve_if_eligible(pool, &account.id)
            .unwrap()
            .unwrap();
        stripe_auto_reload::attach_payment_intent(pool, &attempt.id, payment_intent_id).unwrap()
    }

    fn stripe_intent_event(
        attempt: &StripeAutoReloadAttempt,
        event_id: &str,
        event_type: &str,
        status: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": event_id,
            "type": event_type,
            "data": {
                "object": {
                    "id": attempt.stripe_payment_intent_id,
                    "object": "payment_intent",
                    "amount": attempt.amount_cents,
                    "amount_received": if status == "succeeded" {
                        attempt.amount_cents
                    } else {
                        0
                    },
                    "currency": attempt.currency,
                    "customer": attempt.stripe_customer_id,
                    "payment_method": attempt.stripe_payment_method_id,
                    "status": status,
                    "latest_charge": "ch_auto_reload",
                    "metadata": {
                        "bluey_kind": "auto_topup",
                        "bluey_auto_reload_version": "v1",
                        "bluey_auto_reload_attempt_id": attempt.id,
                        "bluey_account_id": attempt.account_id,
                        "bluey_amount_cents": attempt.amount_cents.to_string()
                    }
                }
            }
        })
    }

    async fn inflight_contains(account_id: &str) -> bool {
        let mu = AUTO_TOPUP_INFLIGHT.get_or_init(Default::default);
        let guard = mu.lock().await;
        guard.contains_key(account_id)
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

    #[tokio::test]
    async fn skip_internal_test_account_even_if_auto_topup_was_already_enabled() {
        let pool = temp_pool();
        let account = Account::create(
            &pool,
            "internal-admin-20260606023943@bluey.sh",
            "password-hash",
        )
        .unwrap();
        Account::mark_email_verified(&pool, &account.id).unwrap();
        Account::save_stripe_checkout_refs(
            &pool,
            &account.id,
            Some("cus_internal"),
            Some("pm_internal"),
        )
        .unwrap();
        let account = Account::update_auto_topup_settings(&pool, &account.id, true, 500, 1500)
            .unwrap()
            .unwrap();

        maybe_spawn(
            pool,
            test_config_with_stripe(),
            account.id.clone(),
            0,
            account.auto_topup_enabled,
            account.auto_topup_threshold_cents,
            account.stripe_customer_id.clone(),
            account.stripe_payment_method_id.clone(),
            account.square_customer_id.clone(),
            account.square_card_id.clone(),
            account.auto_topup_amount_cents,
        );

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !inflight_contains(&account.id).await,
            "internal/test account should skip before reserving an auto reload"
        );
    }

    #[test]
    fn stripe_auto_reload_configuration_fails_closed() {
        let empty = test_config();
        assert_eq!(
            stripe_auto_reload_readiness(&empty, false),
            Err("BLUEY_STRIPE_AUTO_RELOAD_ENABLED is not true")
        );
        assert_eq!(
            stripe_auto_reload_readiness(&empty, true),
            Err("STRIPE_SECRET_KEY is missing")
        );

        let mut missing_webhook = (*empty).clone();
        missing_webhook.stripe_secret_key = Some("sk_test".to_string());
        assert_eq!(
            stripe_auto_reload_readiness(&missing_webhook, true),
            Err("STRIPE_WEBHOOK_SECRET is missing")
        );
        assert!(stripe_auto_reload_readiness(&test_config_with_stripe(), true).is_ok());
    }

    #[test]
    fn succeeded_webhook_credits_auto_reload_exactly_once() {
        let pool = temp_pool();
        let attempt =
            attached_stripe_attempt(&pool, "webhook-success@example.com", "pi_webhook_success");
        let first = stripe_intent_event(
            &attempt,
            "evt_success_1",
            "payment_intent.succeeded",
            "succeeded",
        );
        let second = stripe_intent_event(
            &attempt,
            "evt_success_2",
            "payment_intent.succeeded",
            "succeeded",
        );
        assert!(handle_stripe_payment_intent_event(
            &pool,
            "payment_intent.succeeded",
            "evt_success_1",
            &first
        )
        .unwrap());
        assert!(handle_stripe_payment_intent_event(
            &pool,
            "payment_intent.succeeded",
            "evt_success_2",
            &second
        )
        .unwrap());
        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 1500);
    }

    #[test]
    fn failed_webhook_disables_auto_reload_without_credit() {
        let pool = temp_pool();
        let attempt =
            attached_stripe_attempt(&pool, "webhook-failed@example.com", "pi_webhook_failed");
        let mut event = stripe_intent_event(
            &attempt,
            "evt_failed",
            "payment_intent.payment_failed",
            "requires_payment_method",
        );
        event["data"]["object"]["last_payment_error"] =
            serde_json::json!({"code": "card_declined"});
        assert!(handle_stripe_payment_intent_event(
            &pool,
            "payment_intent.payment_failed",
            "evt_failed",
            &event
        )
        .unwrap());
        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 0);
        assert!(!account.auto_topup_enabled);
        assert!(account.stripe_payment_method_id.is_none());
    }

    #[test]
    fn succeeded_webhook_rejects_identity_mismatch_without_credit() {
        let pool = temp_pool();
        let attempt =
            attached_stripe_attempt(&pool, "webhook-mismatch@example.com", "pi_webhook_mismatch");
        let mut event = stripe_intent_event(
            &attempt,
            "evt_mismatch",
            "payment_intent.succeeded",
            "succeeded",
        );
        event["data"]["object"]["amount"] = serde_json::json!(9999);
        assert!(handle_stripe_payment_intent_event(
            &pool,
            "payment_intent.succeeded",
            "evt_mismatch",
            &event
        )
        .is_err());
        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 0);
    }

    #[test]
    fn reconciliation_detects_refunded_charge_before_credit() {
        let pool = temp_pool();
        let attempt =
            attached_stripe_attempt(&pool, "reconcile-refund@example.com", "pi_reconcile_refund");
        let event = stripe_intent_event(
            &attempt,
            "evt_reconcile",
            "payment_intent.succeeded",
            "succeeded",
        );
        let mut intent = event.pointer("/data/object").unwrap().clone();
        intent["latest_charge"] = serde_json::json!({
            "id": "ch_reconcile_refund",
            "refunded": true,
            "amount_refunded": 1500
        });
        assert_eq!(
            apply_stripe_intent(&pool, &attempt, &intent, None).unwrap(),
            StripeIntentOutcome::Terminal
        );
        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 0);
        assert!(account.billing_restricted);
        let updated = stripe_auto_reload::find_by_id(&pool, &attempt.id)
            .unwrap()
            .unwrap();
        assert_eq!(updated.status, stripe_auto_reload::STATUS_REVERSED);
    }

    #[test]
    fn unrelated_payment_intent_webhook_is_ignored() {
        let pool = temp_pool();
        let event = serde_json::json!({
            "id": "evt_checkout_pi",
            "type": "payment_intent.succeeded",
            "data": {"object": {"id": "pi_checkout", "status": "succeeded"}}
        });
        assert!(!handle_stripe_payment_intent_event(
            &pool,
            "payment_intent.succeeded",
            "evt_checkout_pi",
            &event
        )
        .unwrap());
    }

    #[test]
    fn square_topup_idempotency_key_stays_within_square_limit() {
        let key = square_topup_idempotency_key("833e66ac-0652-43c7-a55e-8d51d9ccc982");
        assert!(key.len() <= 45, "{key}");
        assert!(key.starts_with("bst-"));
    }
}
