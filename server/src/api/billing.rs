//! Billing integration: Stripe compatibility plus Square checkout.
//!
//! v0.2 scope:
//!   - POST /billing/checkout: creates a Stripe Checkout Session for
//!     a $30 reload. Returns the hosted-checkout URL the customer is
//!     redirected to.
//!   - POST /billing/webhook: validates the Stripe-Signature header
//!     and handles `checkout.session.completed` to credit the
//!     customer's balance + record a credit_batches row with the
//!     stripe_charge_id for audit.
//!   - Auto-top-up trigger: post-deduction in /router/complete, if
//!     balance < auto_topup_threshold AND auto_topup_enabled, fire
//!     a charge against the saved PaymentMethod (off-session).
//!
//! Talks Stripe over HTTPS using `reqwest`; no Stripe SDK dependency
//! to keep the binary lean. The endpoints we touch:
//!   - POST /v1/checkout/sessions
//!   - POST /v1/payment_intents (off-session for auto-top-up)
//!
//! All API calls require STRIPE_SECRET_KEY in the server env. If
//! unset, billing endpoints return 503 Service Unavailable so the
//! customer sees a clear "billing not configured" message.

use anyhow::{anyhow, Context, Result};
use axum::{extract::State, http::StatusCode, Extension, Json};
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::config::BillingProvider;
use crate::db::balance;

#[derive(Deserialize)]
pub struct CheckoutRequest {
    /// Reload amount in cents. Must be at least 3000 ($30 minimum).
    pub amount_cents: i64,
}

#[derive(Serialize)]
pub struct CheckoutResponse {
    pub checkout_url: String,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

const MINIMUM_RELOAD_CENTS: i64 = 3000;
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

fn log_safe_stripe_body(body: &serde_json::Value) -> serde_json::Value {
    let mut safe = body.clone();
    redact_stripe_json(&mut safe);
    safe
}

fn redact_stripe_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_sensitive_stripe_log_key(key) {
                    *value = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_stripe_json(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_stripe_json(value);
            }
        }
        _ => {}
    }
}

fn is_sensitive_stripe_log_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("secret")
        || key.contains("token")
        || key.contains("password")
        || key.contains("authorization")
        || key == "url"
        || key.ends_with("_url")
        || key == "client_secret"
        || key == "payment_method"
        || key.ends_with("_payment_method")
}

pub async fn checkout(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, (StatusCode, Json<ApiError>)> {
    match state.config.billing_provider() {
        BillingProvider::Square => square_checkout(state, account, req).await,
        BillingProvider::Stripe => stripe_checkout(state, account, req).await,
    }
}

async fn stripe_checkout(
    state: AppState,
    account: crate::db::accounts::Account,
    req: CheckoutRequest,
) -> Result<Json<CheckoutResponse>, (StatusCode, Json<ApiError>)> {
    let stripe_key = state.config.stripe_secret_key.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                error: "billing not configured".into(),
            }),
        )
    })?;

    if req.amount_cents < MINIMUM_RELOAD_CENTS {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!("minimum reload is ${}", MINIMUM_RELOAD_CENTS / 100),
            }),
        ));
    }

    let account_id_hash = cue_core::account_id_hash_prefix(&account.id);
    tracing::info!(
        account_id_hash = %account_id_hash,
        billing_provider = "stripe",
        amount_cents = req.amount_cents,
        "billing checkout requested"
    );

    // Build Stripe Checkout Session.
    // Form-urlencoded as Stripe's API requires.
    let success_url = format!("{}/account?reload=success", state.config.public_url);
    let cancel_url = format!("{}/account?reload=cancel", state.config.public_url);
    let form = [
        ("mode", "payment"),
        ("payment_method_types[]", "card"),
        ("line_items[0][price_data][currency]", "usd"),
        (
            "line_items[0][price_data][product_data][name]",
            "Bluey credits",
        ),
        (
            "line_items[0][price_data][unit_amount]",
            &req.amount_cents.to_string(),
        ),
        ("line_items[0][quantity]", "1"),
        ("client_reference_id", &account.id),
        ("customer_email", &account.email),
        ("success_url", &success_url),
        ("cancel_url", &cancel_url),
        // Save the PaymentMethod for auto top-up.
        ("payment_intent_data[setup_future_usage]", "off_session"),
        ("metadata[bluey_account_id]", &account.id),
        (
            "metadata[bluey_amount_cents]",
            &req.amount_cents.to_string(),
        ),
    ];

    let resp = reqwest::Client::new()
        .post(stripe_api_url("/v1/checkout/sessions"))
        .basic_auth(stripe_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| {
            // Codex Stage 6 S6.6: log raw upstream details, return
            // sanitized message to the customer.
            tracing::warn!(
                account_id_hash = %account_id_hash,
                billing_provider = "stripe",
                amount_cents = req.amount_cents,
                error = %e,
                "stripe checkout http failed"
            );
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "billing provider unavailable; please retry".into(),
                }),
            )
        })?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let safe_body = log_safe_stripe_body(&body);
        tracing::warn!(
            account_id_hash = %account_id_hash,
            billing_provider = "stripe",
            amount_cents = req.amount_cents,
            stripe_status = %status,
            stripe_body = %safe_body,
            "stripe checkout error"
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "billing checkout failed; please retry".into(),
            }),
        ));
    }
    let url = body
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "stripe response missing url".into(),
                }),
            )
        })?
        .to_string();

    tracing::info!(
        account_id_hash = %account_id_hash,
        billing_provider = "stripe",
        amount_cents = req.amount_cents,
        stripe_checkout_id = %body.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"),
        "billing checkout created"
    );

    Ok(Json(CheckoutResponse { checkout_url: url }))
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

fn square_missing(message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ApiError {
            error: message.to_string(),
        }),
    )
}

fn build_square_payment_link_body(
    public_url: &str,
    location_id: &str,
    account_id: &str,
    customer_email: &str,
    amount_cents: i64,
) -> serde_json::Value {
    serde_json::json!({
        "idempotency_key": format!("bluey-reload-{account_id}-{amount_cents}-{}", uuid::Uuid::new_v4()),
        "order": {
            "location_id": location_id,
            "reference_id": square_reload_reference_id(account_id),
            "metadata": {
                "bluey_account_id": account_id,
                "bluey_amount_cents": amount_cents.to_string()
            },
            "line_items": [{
                "name": "Bluey credits",
                "quantity": "1",
                "base_price_money": {
                    "amount": amount_cents,
                    "currency": "USD"
                }
            }]
        },
        "checkout_options": {
            "redirect_url": format!("{}/account?reload=success", public_url),
            "ask_for_shipping_address": false
        },
        "pre_populated_data": {
            "buyer_email": customer_email
        },
        "payment_note": "Bluey credit reload"
    })
}

fn square_reload_reference_id(account_id: &str) -> String {
    let compact = account_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(32)
        .collect::<String>();
    format!("br_{compact}")
}

fn square_account_id_from_reference(reference_id: &str) -> Option<String> {
    if let Some(account_id) = reference_id.strip_prefix("bluey_reload:") {
        return Some(account_id.to_string());
    }

    let compact = reference_id.strip_prefix("br_")?;
    if compact.len() == 32 && compact.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!(
            "{}-{}-{}-{}-{}",
            &compact[0..8],
            &compact[8..12],
            &compact[12..16],
            &compact[16..20],
            &compact[20..32]
        ));
    }

    None
}

fn log_safe_square_body(body: &serde_json::Value) -> serde_json::Value {
    let mut safe = body.clone();
    redact_stripe_json(&mut safe);
    safe
}

async fn square_checkout(
    state: AppState,
    account: crate::db::accounts::Account,
    req: CheckoutRequest,
) -> Result<Json<CheckoutResponse>, (StatusCode, Json<ApiError>)> {
    if req.amount_cents < MINIMUM_RELOAD_CENTS {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!("minimum reload is ${}", MINIMUM_RELOAD_CENTS / 100),
            }),
        ));
    }

    let square = state.config.square_config();
    let access_token = square
        .access_token
        .as_deref()
        .ok_or_else(|| square_missing("Square billing not configured"))?;
    let location_id = square
        .location_id
        .as_deref()
        .ok_or_else(|| square_missing("Square location not configured"))?;

    let account_id_hash = cue_core::account_id_hash_prefix(&account.id);
    tracing::info!(
        account_id_hash = %account_id_hash,
        billing_provider = "square",
        amount_cents = req.amount_cents,
        "billing checkout requested"
    );

    let body = build_square_payment_link_body(
        &state.config.public_url,
        location_id,
        &account.id,
        &account.email,
        req.amount_cents,
    );

    let resp = reqwest::Client::new()
        .post(square_api_url(&square, "/v2/online-checkout/payment-links"))
        .bearer_auth(access_token)
        .header("Square-Version", SQUARE_API_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                billing_provider = "square",
                amount_cents = req.amount_cents,
                error = %e,
                "square checkout http failed"
            );
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "billing provider unavailable; please retry".into(),
                }),
            )
        })?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let safe_body = log_safe_square_body(&body);
        tracing::warn!(
            account_id_hash = %account_id_hash,
            billing_provider = "square",
            amount_cents = req.amount_cents,
            square_status = %status,
            square_body = %safe_body,
            "square checkout error"
        );
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "billing checkout failed; please retry".into(),
            }),
        ));
    }

    let url = body
        .pointer("/payment_link/url")
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.pointer("/payment_link/long_url")
                .and_then(|v| v.as_str())
        })
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "Square checkout response missing url".into(),
                }),
            )
        })?
        .to_string();

    tracing::info!(
        account_id_hash = %account_id_hash,
        billing_provider = "square",
        amount_cents = req.amount_cents,
        square_payment_link_id = %body
            .pointer("/payment_link/id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        "billing checkout created"
    );

    Ok(Json(CheckoutResponse { checkout_url: url }))
}

pub async fn webhook(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    if headers.contains_key("x-square-hmacsha256-signature")
        || matches!(state.config.billing_provider(), BillingProvider::Square)
    {
        return square_webhook_impl(state, headers, body).await;
    }
    stripe_webhook_impl(state, headers, body).await
}

pub async fn square_webhook(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    square_webhook_impl(state, headers, body).await
}

async fn stripe_webhook_impl(
    state: AppState,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    let webhook_secret = state
        .config
        .stripe_webhook_secret
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let sig_header = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Err(e) = verify_stripe_signature(webhook_secret, sig_header, &body) {
        tracing::warn!(error = %e, "stripe webhook signature rejected");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let event: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let event_id = event
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Idempotency: skip if we've already processed this event.
    let conn = state
        .pool
        .get()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let already: Option<String> = conn
        .query_row(
            "SELECT processed_at FROM stripe_webhook_events WHERE event_id = ?1",
            rusqlite::params![event_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    if already.is_some() {
        return Ok(StatusCode::OK);
    }
    let _ = conn.execute(
        "INSERT OR IGNORE INTO stripe_webhook_events (event_id, type, body) VALUES (?1, ?2, ?3)",
        rusqlite::params![event_id, event_type, &body],
    );

    if event_type == "checkout.session.completed" {
        if let Err(e) = handle_checkout_completed(&state, &event).await {
            tracing::error!(error = %e, event_id, "checkout.session.completed handler failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    let _ = conn.execute(
        "UPDATE stripe_webhook_events SET processed_at = datetime('now') WHERE event_id = ?1",
        rusqlite::params![event_id],
    );

    Ok(StatusCode::OK)
}

async fn square_webhook_impl(
    state: AppState,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<StatusCode, StatusCode> {
    let square = state.config.square_config();
    let webhook_secret = square
        .webhook_signature_key
        .as_deref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let notification_url = square
        .webhook_notification_url
        .as_deref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let sig_header = headers
        .get("x-square-hmacsha256-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Err(e) = verify_square_signature(webhook_secret, notification_url, &body, sig_header) {
        tracing::warn!(error = %e, "square webhook signature rejected");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let event: serde_json::Value =
        serde_json::from_str(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let event_id = event
        .get("event_id")
        .or_else(|| event.get("id"))
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let stored_event_id = format!("square:{event_id}");

    let conn = state
        .pool
        .get()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let already: Option<String> = conn
        .query_row(
            "SELECT processed_at FROM stripe_webhook_events WHERE event_id = ?1",
            rusqlite::params![&stored_event_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    if already.is_some() {
        return Ok(StatusCode::OK);
    }
    let _ = conn.execute(
        "INSERT OR IGNORE INTO stripe_webhook_events (event_id, type, body) VALUES (?1, ?2, ?3)",
        rusqlite::params![&stored_event_id, event_type, &body],
    );
    drop(conn);

    if matches!(
        event_type,
        "order.updated" | "payment.updated" | "payment.created"
    ) {
        if let Err(e) = handle_square_payment_event(&state, &event).await {
            tracing::error!(error = %e, event_id = %stored_event_id, "square webhook handler failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    if let Ok(conn) = state.pool.get() {
        let _ = conn.execute(
            "UPDATE stripe_webhook_events SET processed_at = datetime('now') WHERE event_id = ?1",
            rusqlite::params![&stored_event_id],
        );
    }

    Ok(StatusCode::OK)
}

/// Extract the PaymentIntent id from a Stripe session.payment_intent
/// field, which Stripe sends as either a bare string (the id) or an
/// expanded object (with .id + other fields).
fn extract_payment_intent_id(session: &serde_json::Value) -> Option<String> {
    let pi = session.get("payment_intent")?;
    if let Some(s) = pi.as_str() {
        return Some(s.to_string());
    }
    if let Some(id) = pi.get("id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    None
}

/// Extract the saved PaymentMethod id from a Stripe session, trying
/// (in order):
///
///   1. session.payment_intent.payment_method (when PI is expanded)
///   2. session.setup_intent.payment_method (alternate flow)
///
/// Returns None if neither is present; in that case the caller should
/// fetch the PaymentIntent via /v1/payment_intents/{id} to expand it.
fn extract_payment_method_id_from_session(session: &serde_json::Value) -> Option<String> {
    if let Some(pm) = session
        .pointer("/payment_intent/payment_method")
        .and_then(|v| v.as_str())
    {
        return Some(pm.to_string());
    }
    if let Some(pm) = session
        .pointer("/setup_intent/payment_method")
        .and_then(|v| v.as_str())
    {
        return Some(pm.to_string());
    }
    None
}

/// Fetch a PaymentIntent from Stripe and read its payment_method field.
/// Used when the webhook's session.payment_intent is a bare string
/// (Stripe's default Checkout webhook shape) so we can still persist
/// stripe_payment_method_id for off-session auto top-up.
async fn fetch_payment_method_from_stripe(
    stripe_key: &str,
    payment_intent_id: &str,
) -> Result<Option<String>> {
    let url = stripe_api_url(&format!("/v1/payment_intents/{payment_intent_id}"));
    let resp = reqwest::Client::new()
        .get(&url)
        .basic_auth(stripe_key, Some(""))
        .send()
        .await
        .context("stripe payment_intents.retrieve http")?;
    if !resp.status().is_success() {
        return Err(anyhow!(
            "stripe payment_intents.retrieve {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        ));
    }
    let body: serde_json::Value = resp.json().await.context("stripe payment_intents json")?;
    Ok(body
        .get("payment_method")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

async fn handle_checkout_completed(state: &AppState, event: &serde_json::Value) -> Result<()> {
    let session = event
        .pointer("/data/object")
        .ok_or_else(|| anyhow!("no data.object"))?;
    let account_id = session
        .get("client_reference_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            session
                .pointer("/metadata/bluey_account_id")
                .and_then(|v| v.as_str())
        })
        .ok_or_else(|| anyhow!("no account_id in session"))?
        .to_string();
    let account_id_hash = cue_core::account_id_hash_prefix(&account_id);
    let amount_cents = session
        .pointer("/metadata/bluey_amount_cents")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| session.get("amount_total").and_then(|v| v.as_i64()))
        .ok_or_else(|| anyhow!("no amount in session"))?;

    // Codex round-2 blocker 1: normalize payment_intent for both string
    // AND expanded-object shapes so credit idempotency always gets the
    // PI id (defending against duplicate-charge replay) regardless of
    // which webhook expansion mode Stripe is using.
    let payment_intent_id = extract_payment_intent_id(session);

    let credited = balance::credit(
        &state.pool,
        &account_id,
        amount_cents,
        payment_intent_id.as_deref(),
    )
    .context("credit account")?;

    // Codex round-2 blocker 1: persist stripe_payment_method_id for
    // auto top-up. Try the expanded session first; if PI was a bare
    // string, fetch /v1/payment_intents/{id} to read .payment_method.
    let mut payment_method_id = extract_payment_method_id_from_session(session);
    if payment_method_id.is_none() {
        if let (Some(pi_id), Some(stripe_key)) = (
            payment_intent_id.as_deref(),
            state.config.stripe_secret_key.as_deref(),
        ) {
            match fetch_payment_method_from_stripe(stripe_key, pi_id).await {
                Ok(Some(pm)) => payment_method_id = Some(pm),
                Ok(None) => {
                    tracing::warn!(
                        account_id_hash = %account_id_hash,
                        payment_intent_id = pi_id,
                        "PaymentIntent retrieve returned no payment_method; auto top-up will rely on customer-default"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        account_id_hash = %account_id_hash,
                        payment_intent_id = pi_id,
                        error = %e,
                        "PaymentIntent retrieve failed; will use customer-default at auto top-up time"
                    );
                }
            }
        }
    }

    if let Ok(conn) = state.pool.get() {
        let customer_id = session.get("customer").and_then(|v| v.as_str());
        if customer_id.is_some() || payment_method_id.is_some() {
            let _ = conn.execute(
                "UPDATE accounts
                    SET stripe_customer_id = COALESCE(?1, stripe_customer_id),
                        stripe_payment_method_id = COALESCE(?2, stripe_payment_method_id)
                  WHERE id = ?3",
                rusqlite::params![customer_id, payment_method_id.as_deref(), &account_id],
            );
        }
    }

    if !credited {
        tracing::info!(
            account_id_hash = %account_id_hash,
            payment_intent_id = %payment_intent_id.as_deref().unwrap_or("unknown"),
            "checkout.session.completed: charge already credited, no-op"
        );
    } else {
        tracing::info!(
            account_id_hash = %account_id_hash,
            amount_cents,
            payment_intent_id = %payment_intent_id.as_deref().unwrap_or("unknown"),
            "credited from Stripe webhook"
        );
    }
    Ok(())
}

async fn handle_square_payment_event(state: &AppState, event: &serde_json::Value) -> Result<()> {
    let Some((account_id, amount_cents, payment_id)) = extract_square_credit(event)? else {
        tracing::debug!("square webhook did not contain a completed Bluey reload");
        return Ok(());
    };

    let charge_id = format!("square:{payment_id}");
    let credited = balance::credit(&state.pool, &account_id, amount_cents, Some(&charge_id))
        .context("credit account from Square")?;
    if !credited {
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            square_payment_id = %payment_id,
            "Square payment already credited, no-op"
        );
    } else {
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            amount_cents,
            square_payment_id = %payment_id,
            "credited from Square webhook"
        );
    }
    Ok(())
}

fn extract_square_credit(event: &serde_json::Value) -> Result<Option<(String, i64, String)>> {
    if let Some(order) = event.pointer("/data/object/order") {
        let state = order.get("state").and_then(|v| v.as_str()).unwrap_or("");
        if state != "COMPLETED" {
            return Ok(None);
        }
        let account_id = order
            .pointer("/metadata/bluey_account_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                order
                    .get("reference_id")
                    .and_then(|v| v.as_str())
                    .and_then(square_account_id_from_reference)
            })
            .ok_or_else(|| anyhow!("Square order missing bluey account metadata"))?;
        let amount_cents = order
            .pointer("/metadata/bluey_amount_cents")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| {
                order
                    .pointer("/total_money/amount")
                    .and_then(|v| v.as_i64())
            })
            .ok_or_else(|| anyhow!("Square order missing amount"))?;
        let payment_id = order
            .pointer("/tenders/0/payment_id")
            .and_then(|v| v.as_str())
            .or_else(|| order.get("id").and_then(|v| v.as_str()))
            .ok_or_else(|| anyhow!("Square order missing id/payment_id"))?
            .to_string();
        return Ok(Some((account_id, amount_cents, payment_id)));
    }

    if let Some(payment) = event.pointer("/data/object/payment") {
        let status = payment.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status != "COMPLETED" {
            return Ok(None);
        }
        let account_id = payment
            .get("reference_id")
            .and_then(|v| v.as_str())
            .and_then(square_account_id_from_reference)
            .ok_or_else(|| anyhow!("Square payment missing bluey reference_id"))?;
        let amount_cents = payment
            .pointer("/amount_money/amount")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow!("Square payment missing amount"))?;
        let payment_id = payment
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Square payment missing id"))?
            .to_string();
        return Ok(Some((account_id, amount_cents, payment_id)));
    }

    Ok(None)
}

/// Verify Stripe `t=...,v1=...` signature header against the body.
/// Implements the standard scheme: signed_payload = "t.body",
/// HMAC-SHA256 with the webhook secret, constant-time compared to v1.
///
/// Codex Stage 6 S6.5:
///   - Constant-time signature compare via subtle::ConstantTimeEq.
///   - Timestamp tolerance: reject events outside +/- 5 minutes of now.
fn verify_stripe_signature(secret: &str, sig_header: &str, body: &str) -> Result<()> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use subtle::ConstantTimeEq;

    let mut timestamp = None;
    let mut signatures: Vec<&str> = Vec::new();
    for kv in sig_header.split(',') {
        let mut it = kv.splitn(2, '=');
        let k = it.next().unwrap_or("").trim();
        let v = it.next().unwrap_or("").trim();
        match k {
            "t" => timestamp = Some(v),
            "v1" => signatures.push(v),
            _ => {}
        }
    }
    let t = timestamp.ok_or_else(|| anyhow!("no timestamp in Stripe-Signature"))?;
    if signatures.is_empty() {
        return Err(anyhow!("no v1 signature in Stripe-Signature"));
    }

    // Reject events outside +/- 5 minutes (Stripe-recommended tolerance).
    let event_ts: i64 = t
        .parse()
        .map_err(|e| anyhow!("invalid timestamp {t}: {e}"))?;
    let now = chrono::Utc::now().timestamp();
    let skew = (now - event_ts).abs();
    const TOLERANCE_SECS: i64 = 300;
    if skew > TOLERANCE_SECS {
        return Err(anyhow!(
            "timestamp outside tolerance ({skew}s > {TOLERANCE_SECS}s)"
        ));
    }

    let signed_payload = format!("{t}.{body}");
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow!("hmac key: {e}"))?;
    mac.update(signed_payload.as_bytes());
    let computed = mac.finalize().into_bytes();
    let computed_hex = hex::encode(computed);
    let computed_bytes = computed_hex.as_bytes();
    let mut any_match = false;
    for sig in &signatures {
        let sig_bytes = sig.as_bytes();
        if sig_bytes.len() != computed_bytes.len() {
            continue;
        }
        if sig_bytes.ct_eq(computed_bytes).into() {
            any_match = true;
        }
    }
    if !any_match {
        return Err(anyhow!("signature mismatch"));
    }
    Ok(())
}

fn verify_square_signature(
    signature_key: &str,
    notification_url: &str,
    body: &str,
    signature_header: &str,
) -> Result<()> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use subtle::ConstantTimeEq;

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(signature_key.as_bytes())
        .map_err(|e| anyhow!("hmac key: {e}"))?;
    mac.update(notification_url.as_bytes());
    mac.update(body.as_bytes());
    let computed = mac.finalize().into_bytes();
    let computed_b64 = base64::engine::general_purpose::STANDARD.encode(computed);

    let expected = computed_b64.as_bytes();
    let actual = signature_header.trim().as_bytes();
    if actual.len() != expected.len() || !bool::from(actual.ct_eq(expected)) {
        return Err(anyhow!("signature mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_verifies_with_correct_secret() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let secret = "whsec_test_abc";
        let body = r#"{"id":"evt_1","type":"checkout.session.completed"}"#;
        let t = chrono::Utc::now().timestamp().to_string();
        let t = t.as_str();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.{body}").as_bytes());
        let v1 = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={t},v1={v1}");
        assert!(verify_stripe_signature(secret, &header, body).is_ok());
    }

    #[test]
    fn signature_verifies_when_any_v1_signature_matches() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let secret = "whsec_test_rotation";
        let body = r#"{"id":"evt_rotation","type":"checkout.session.completed"}"#;
        let t = chrono::Utc::now().timestamp().to_string();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.{body}").as_bytes());
        let valid = hex::encode(mac.finalize().into_bytes());
        let invalid = "0".repeat(64);
        let header = format!("t={t},v1={invalid},v1={valid}");
        assert!(verify_stripe_signature(secret, &header, body).is_ok());
    }

    #[test]
    fn stripe_log_body_redacts_sensitive_fields() {
        let body = serde_json::json!({
            "error": {
                "message": "bad request",
                "url": "https://checkout.stripe.com/c/pay/secret",
                "client_secret": "pi_secret_123",
                "payment_method": "pm_secret"
            }
        });

        let safe = log_safe_stripe_body(&body).to_string();
        assert!(!safe.contains("checkout.stripe.com"));
        assert!(!safe.contains("pi_secret_123"));
        assert!(!safe.contains("pm_secret"));
        assert!(safe.contains("<redacted>"));
    }

    #[test]
    fn square_payment_link_body_carries_bluey_metadata() {
        let body = build_square_payment_link_body(
            "https://bluey.sh",
            "LOC_TEST",
            "acct-123",
            "user@example.com",
            3000,
        );
        assert_eq!(body.pointer("/order/location_id").unwrap(), "LOC_TEST");
        assert_eq!(body.pointer("/order/reference_id").unwrap(), "br_acct123");
        assert_eq!(
            body.pointer("/order/metadata/bluey_account_id").unwrap(),
            "acct-123"
        );
        assert_eq!(
            body.pointer("/order/line_items/0/base_price_money/amount")
                .unwrap(),
            3000
        );
        assert_eq!(
            body.pointer("/checkout_options/redirect_url").unwrap(),
            "https://bluey.sh/account?reload=success"
        );
    }

    #[test]
    fn square_reference_id_stays_within_square_limit() {
        let account_id = "833e66ac-0652-43c7-a55e-8d51d9ccc982";
        let reference_id = square_reload_reference_id(account_id);
        assert_eq!(reference_id, "br_833e66ac065243c7a55e8d51d9ccc982");
        assert!(reference_id.len() <= 40);
        assert_eq!(
            square_account_id_from_reference(&reference_id).as_deref(),
            Some(account_id)
        );
    }

    #[test]
    fn square_signature_verifies_with_correct_secret() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = "sq-webhook-secret";
        let url = "https://bluey.sh/billing/square/webhook";
        let body = r#"{"event_id":"evt_square","type":"order.updated"}"#;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(url.as_bytes());
        mac.update(body.as_bytes());
        let sig = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        assert!(verify_square_signature(secret, url, body, &sig).is_ok());
        assert!(verify_square_signature("wrong", url, body, &sig).is_err());
    }

    #[test]
    fn square_order_extracts_credit_from_completed_order() {
        let event = serde_json::json!({
            "event_id": "evt1",
            "type": "order.updated",
            "data": {
                "object": {
                    "order": {
                        "id": "order_1",
                        "state": "COMPLETED",
                        "reference_id": "bluey_reload:acct-123",
                        "metadata": {
                            "bluey_account_id": "acct-123",
                            "bluey_amount_cents": "3000"
                        },
                        "total_money": {"amount": 3000, "currency": "USD"},
                        "tenders": [{"payment_id": "payment_1"}]
                    }
                }
            }
        });

        let extracted = extract_square_credit(&event).unwrap().unwrap();
        assert_eq!(extracted.0, "acct-123");
        assert_eq!(extracted.1, 3000);
        assert_eq!(extracted.2, "payment_1");
    }

    #[test]
    fn square_order_ignores_non_completed_order() {
        let event = serde_json::json!({
            "event_id": "evt1",
            "type": "order.updated",
            "data": {
                "object": {
                    "order": {
                        "id": "order_1",
                        "state": "OPEN",
                        "reference_id": "bluey_reload:acct-123",
                        "total_money": {"amount": 3000, "currency": "USD"}
                    }
                }
            }
        });

        assert!(extract_square_credit(&event).unwrap().is_none());
    }

    #[test]
    fn square_payment_extracts_credit_from_compact_reference() {
        let event = serde_json::json!({
            "event_id": "evt_payment",
            "type": "payment.updated",
            "data": {
                "object": {
                    "payment": {
                        "id": "payment_1",
                        "status": "COMPLETED",
                        "reference_id": "br_833e66ac065243c7a55e8d51d9ccc982",
                        "amount_money": {"amount": 3000, "currency": "USD"}
                    }
                }
            }
        });

        let extracted = extract_square_credit(&event).unwrap().unwrap();
        assert_eq!(extracted.0, "833e66ac-0652-43c7-a55e-8d51d9ccc982");
        assert_eq!(extracted.1, 3000);
        assert_eq!(extracted.2, "payment_1");
    }

    #[test]
    fn signature_rejects_wrong_secret() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let body = r#"{"id":"evt_1"}"#;
        let t = chrono::Utc::now().timestamp().to_string();
        let t = t.as_str();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"correct").unwrap();
        mac.update(format!("{t}.{body}").as_bytes());
        let v1 = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={t},v1={v1}");
        assert!(verify_stripe_signature("wrong", &header, body).is_err());
    }

    #[test]
    fn signature_rejects_stale_timestamp() {
        // Codex Stage 6 S6.5: reject events outside +/- 5 minutes.
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let secret = "whsec_stale";
        let body = r#"{"id":"evt_stale"}"#;
        // 2023 timestamp — far outside tolerance.
        let t = "1700000000";
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.{body}").as_bytes());
        let v1 = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={t},v1={v1}");
        let res = verify_stripe_signature(secret, &header, body);
        assert!(res.is_err());
        let msg = format!("{}", res.unwrap_err());
        assert!(
            msg.contains("tolerance"),
            "expected tolerance error, got {msg}"
        );
    }
}

#[derive(serde::Serialize)]
pub struct PortalResponse {
    pub portal_url: String,
}

/// Codex Stage 14: Stripe Customer Portal session.
/// Customer clicks "manage billing" -> redirected to Stripe-hosted UI
/// for managing their saved card / canceling auto top-up / viewing
/// invoices. Returns the session URL.
pub async fn portal(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<PortalResponse>, (StatusCode, Json<ApiError>)> {
    if matches!(state.config.billing_provider(), BillingProvider::Square) {
        // Square does not provide a Stripe-style hosted customer portal.
        // Send customers to Bluey's account page where we can show
        // reloads, receipts, and future card-management UX.
        return Ok(Json(PortalResponse {
            portal_url: format!("{}/account?billing=square", state.config.public_url),
        }));
    }
    stripe_portal(state, account).await
}

async fn stripe_portal(
    state: AppState,
    account: crate::db::accounts::Account,
) -> Result<Json<PortalResponse>, (StatusCode, Json<ApiError>)> {
    let stripe_key = state.config.stripe_secret_key.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                error: "billing not configured".into(),
            }),
        )
    })?;
    let customer_id = account.stripe_customer_id.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "no Stripe customer on file; complete a reload first".into(),
            }),
        )
    })?;
    let return_url = format!("{}/account", state.config.public_url);
    let form = [
        ("customer", customer_id.as_str()),
        ("return_url", return_url.as_str()),
    ];
    let resp = reqwest::Client::new()
        .post(stripe_api_url("/v1/billing_portal/sessions"))
        .basic_auth(stripe_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "stripe portal http failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "billing provider unavailable; please retry".into(),
                }),
            )
        })?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let safe_body = log_safe_stripe_body(&body);
        tracing::warn!(stripe_status = %status, stripe_body = %safe_body, "stripe portal error");
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "billing portal session failed; please retry".into(),
            }),
        ));
    }
    let url = body
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "stripe portal response missing url".into(),
                }),
            )
        })?
        .to_string();
    Ok(Json(PortalResponse { portal_url: url }))
}
