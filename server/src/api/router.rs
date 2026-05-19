//! Real router endpoints: managed dispatch + atomic deduction.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::{balance, usage::UsageEvent};
use crate::pricing;
use crate::routing;

#[derive(Deserialize)]
pub struct CompleteRequest {
    pub system: String,
    pub user: String,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    pub lane: String,
    #[serde(default)]
    pub estimated_input_tokens: Option<i64>,
}

#[derive(Serialize)]
pub struct CompleteResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload_url: Option<String>,
}

pub async fn complete(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<CompleteRequest>,
) -> Result<Json<CompleteResponse>, (StatusCode, Json<ApiError>)> {
    // 1. Resolve lane → provider+model.
    let (provider, model) = routing::resolve_route(&req.lane);
    let pricing_entry = pricing::lookup(provider, model).ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("no pricing for {provider}/{model}"),
                ..Default::default()
            }),
        )
    })?;

    // 2. Estimate cost ceiling for the entry check.
    let max_out = req.max_tokens.unwrap_or(2048) as i64;
    let est_in = req.estimated_input_tokens.unwrap_or_else(|| {
        // Crude fallback: ~4 chars/token
        ((req.system.len() + req.user.len()) as i64) / 4
    });
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_in, max_out);

    let on_trial = account.trial_seconds_remaining > 0;

    // 3. Entry check — unless the account is still on the free trial.
    if !on_trial {
        let can = balance::can_afford(&state.pool, &account.id, est_cost).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: format!("balance: {e}"), ..Default::default() }),
            )
        })?;
        if !can {
            let bal = balance::current_balance(&state.pool, &account.id).unwrap_or(0);
            return Err((
                StatusCode::PAYMENT_REQUIRED,
                Json(ApiError {
                    error: "insufficient balance".into(),
                    balance_cents: Some(bal),
                    estimated_cost_cents: Some(est_cost),
                    reason: Some("insufficient_balance".into()),
                    reload_url: Some(format!("{}/reload", state.config.public_url)),
                }),
            ));
        }
    }

    // 4. Dispatch to upstream provider.
    let started = Instant::now();
    let result = routing::complete(
        &state.config.upstream,
        provider,
        model,
        &req.system,
        &req.user,
        req.max_tokens,
        req.temperature,
    )
    .await;
    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis() as i64;

    let comp = result.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: format!("upstream: {e}"),
                ..Default::default()
            }),
        )
    })?;

    // 5. Compute actual cost from real token counts.
    let (bluey_cost, customer_cost) =
        pricing::compute_cost(pricing_entry, comp.input_tokens, comp.output_tokens);

    // 6. Charge: trial decrement OR balance deduction.
    let trial_remaining = if on_trial {
        balance::consume_trial_seconds(&state.pool, &account.id, elapsed_ms).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: format!("trial: {e}"), ..Default::default() }),
            )
        })?
    } else {
        let ok = balance::deduct(&state.pool, &account.id, customer_cost).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { error: format!("deduct: {e}"), ..Default::default() }),
            )
        })?;
        // If !ok here we'd be in a "completed upstream call but couldn't
        // charge" state. Bluey eats the overrun rather than putting the
        // customer in the red (per DECISIONS.md hard-stop guarantee).
        if !ok {
            tracing::warn!(
                account_id = %account.id,
                cost_cents = customer_cost,
                "post-completion deduct failed; bluey absorbs overrun"
            );
        }
        account.trial_seconds_remaining
    };

    let balance_after = balance::current_balance(&state.pool, &account.id).unwrap_or(0);

    // 7. Record usage event.
    let event = UsageEvent {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind: "llm".into(),
        task_type: None,
        lane: Some(req.lane.clone()),
        provider: Some(comp.provider.clone()),
        model: Some(comp.model.clone()),
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        latency_ms: elapsed_ms,
        cost_cents_to_bluey: bluey_cost,
        cost_cents_to_customer: customer_cost,
        was_speculative: false,
        was_fallback: false,
    };
    if let Err(e) = crate::db::usage::record(&state.pool, &account.id, &event) {
        tracing::warn!(error = %e, "failed to record usage event");
    }

    Ok(Json(CompleteResponse {
        text: comp.text,
        provider: comp.provider,
        model: comp.model,
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        cost_cents: customer_cost,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_remaining,
    }))
}

impl Default for ApiError {
    fn default() -> Self {
        Self {
            error: String::new(),
            balance_cents: None,
            estimated_cost_cents: None,
            reason: None,
            reload_url: None,
        }
    }
}

// ─── Embed + transcribe (stubs; real impls land later) ──────────────────

#[derive(Deserialize)]
pub struct EmbedRequest {
    pub text: String,
    pub model: Option<String>,
}

#[derive(Serialize)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
}

pub async fn embed(
    State(_state): State<AppState>,
    Extension(_account): Extension<AuthedAccount>,
    Json(_req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Deserialize)]
pub struct TranscribeRequest {
    pub audio_base64: String,
    pub language: Option<String>,
    pub format: Option<String>,
}

#[derive(Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    pub provider: String,
    pub duration_ms: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
}

pub async fn transcribe(
    State(_state): State<AppState>,
    Extension(_account): Extension<AuthedAccount>,
    Json(_req): Json<TranscribeRequest>,
) -> Result<Json<TranscribeResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}
