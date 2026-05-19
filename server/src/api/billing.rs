//! Stripe integration: Checkout Sessions + webhook handling.
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
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::AuthedAccount;
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

pub async fn checkout(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<CheckoutRequest>,
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
        .post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(stripe_key, Some(""))
        .form(&form)
        .send()
        .await
        .map_err(|e| {
            // Codex Stage 6 S6.6: log raw upstream details, return
            // sanitized message to the customer.
            tracing::warn!(error = %e, "stripe checkout http failed");
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
        tracing::warn!(stripe_status = %status, stripe_body = %body, "stripe checkout error");
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

    Ok(Json(CheckoutResponse { checkout_url: url }))
}

pub async fn webhook(
    State(state): State<AppState>,
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
        if let Err(e) = handle_checkout_completed(&state, &event) {
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

fn handle_checkout_completed(state: &AppState, event: &serde_json::Value) -> Result<()> {
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
    let amount_cents = session
        .pointer("/metadata/bluey_amount_cents")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| session.get("amount_total").and_then(|v| v.as_i64()))
        .ok_or_else(|| anyhow!("no amount in session"))?;

    let payment_intent_id = session
        .get("payment_intent")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let credited = balance::credit(
        &state.pool,
        &account_id,
        amount_cents,
        payment_intent_id.as_deref(),
    )
    .context("credit account")?;

    // Codex Stage 6 S6.3: persist BOTH the customer id AND the payment
    // method id for auto top-up. The setup_future_usage=off_session
    // hint at checkout time means Stripe attaches the PaymentMethod to
    // the customer. We read it from the session\'s payment_intent_data
    // (when expanded) or fall back to charging via customer-default at
    // top-up time.
    if let Ok(conn) = state.pool.get() {
        let customer_id = session.get("customer").and_then(|v| v.as_str());
        let payment_method_id = session
            .pointer("/payment_intent/payment_method")
            .and_then(|v| v.as_str())
            .or_else(|| {
                session
                    .get("setup_intent")
                    .and_then(|v| v.get("payment_method"))
                    .and_then(|v| v.as_str())
            });
        if customer_id.is_some() || payment_method_id.is_some() {
            let _ = conn.execute(
                "UPDATE accounts
                    SET stripe_customer_id = COALESCE(?1, stripe_customer_id),
                        stripe_payment_method_id = COALESCE(?2, stripe_payment_method_id)
                  WHERE id = ?3",
                rusqlite::params![customer_id, payment_method_id, &account_id],
            );
        }
    }

    if !credited {
        tracing::info!(
            account_id,
            "checkout.session.completed: charge already credited, no-op"
        );
    }

    tracing::info!(account_id, amount_cents, "credited from Stripe webhook");
    Ok(())
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
