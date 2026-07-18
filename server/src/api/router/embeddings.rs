use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use super::{
    billing_restricted_error, release_and_capacity_error, release_and_upstream_spend_guard_check,
    ApiError, AppState,
};
use crate::auth::AuthedAccount;
use crate::db::{balance, idempotency, usage::UsageEvent};
use crate::{pricing, routing};

#[derive(Deserialize)]
pub struct EmbedRequest {
    pub request_id: String,
    pub input: String,
    /// Optional model override. Defaults to text-embedding-3-small.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EmbedResponse {
    pub vector: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    #[serde(default)]
    pub trial_seconds_remaining: i64,
}

#[derive(Deserialize)]
pub struct EmbedBatchRequest {
    pub request_id: String,
    pub inputs: Vec<String>,
    /// Optional model override. Defaults to text-embedding-3-small.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EmbedBatchResponse {
    pub vectors: Vec<Vec<f32>>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    #[serde(default)]
    pub trial_seconds_remaining: i64,
}

pub async fn embed(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    Json(req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, Json<ApiError>)> {
    let batch = embed_batch_inner(
        &state,
        &account,
        &trace_id,
        EmbedBatchRequest {
            request_id: req.request_id,
            inputs: vec![req.input],
            model: req.model,
        },
    )
    .await?;
    let vector = batch.vectors.into_iter().next().ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "upstream embed response was empty".into(),
                reason: Some("upstream_error".into()),
                ..Default::default()
            }),
        )
    })?;
    Ok(Json(EmbedResponse {
        vector,
        provider: batch.provider,
        model: batch.model,
        input_tokens: batch.input_tokens,
        cost_cents: batch.cost_cents,
        balance_cents_after: batch.balance_cents_after,
        trial_seconds_remaining: batch.trial_seconds_remaining,
    }))
}

pub async fn embed_batch(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    Json(req): Json<EmbedBatchRequest>,
) -> Result<Json<EmbedBatchResponse>, (StatusCode, Json<ApiError>)> {
    embed_batch_inner(&state, &account, &trace_id, req)
        .await
        .map(Json)
}

async fn embed_batch_inner(
    state: &AppState,
    account: &crate::db::accounts::Account,
    trace_id: &str,
    req: EmbedBatchRequest,
) -> Result<EmbedBatchResponse, (StatusCode, Json<ApiError>)> {
    if let Some(err) = billing_restricted_error(account) {
        return Err(err);
    }
    if req.request_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "request_id is required and must be non-empty".into(),
                reason: Some("missing_request_id".into()),
                ..Default::default()
            }),
        ));
    }
    if req.inputs.is_empty() || req.inputs.iter().any(|input| input.trim().is_empty()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "inputs must contain at least one non-empty string".into(),
                reason: Some("invalid_input".into()),
                ..Default::default()
            }),
        ));
    }

    // Idempotency reservation (same scheme as /router/complete).
    match idempotency::reserve(&state.pool, &account.id, &req.request_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("idempotency: {e}"),
                ..Default::default()
            }),
        )
    })? {
        idempotency::ReserveOutcome::FreshReservation => { /* fall through */ }
        idempotency::ReserveOutcome::CachedComplete(json) => {
            if let Ok(cached) = serde_json::from_str::<EmbedBatchResponse>(&json) {
                return Ok(cached);
            }
            let cached: EmbedResponse = serde_json::from_str(&json).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("idempotency cache decode: {e}"),
                        ..Default::default()
                    }),
                )
            })?;
            return Ok(EmbedBatchResponse {
                vectors: vec![cached.vector],
                provider: cached.provider,
                model: cached.model,
                input_tokens: cached.input_tokens,
                cost_cents: cached.cost_cents,
                balance_cents_after: cached.balance_cents_after,
                trial_seconds_remaining: cached.trial_seconds_remaining,
            });
        }
        idempotency::ReserveOutcome::InProgress => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "request already in progress".into(),
                    reason: Some("request_in_progress".into()),
                    ..Default::default()
                }),
            ));
        }
        idempotency::ReserveOutcome::CachedFailed => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "previous attempt failed; use a new request_id".into(),
                    reason: Some("request_failed_terminal".into()),
                    ..Default::default()
                }),
            ));
        }
    }

    let provider = "openai";
    let model = req.model.as_deref().unwrap_or("text-embedding-3-small");
    let pricing_entry = pricing::lookup(provider, model).ok_or_else(|| {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("no pricing for {provider}/{model}"),
                ..Default::default()
            }),
        )
    })?;

    if let Err(denied) = state.rate_limiters.check_account_embed(&account.id).await {
        return Err(release_and_capacity_error(
            &state.pool,
            &account.id,
            &req.request_id,
            denied.reason,
            denied.retry_after_secs,
        ));
    }
    // Entry check (skipped on trial).
    let on_trial = account.trial_seconds_remaining > 0;
    let est_in = req
        .inputs
        .iter()
        .map(|input| (input.len() as i64) / 4)
        .sum();
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_in, 0);
    let est_bluey_cost = pricing::estimate_bluey_cost_ceiling(pricing_entry, est_in, 0);
    if let Some(err) = release_and_upstream_spend_guard_check(
        state,
        &account.id,
        &req.request_id,
        est_bluey_cost,
        "embed",
    ) {
        return Err(err);
    }
    if !on_trial {
        let can = balance::can_afford(&state.pool, &account.id, est_cost).map_err(|e| {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("balance: {e}"),
                    ..Default::default()
                }),
            )
        })?;
        if !can {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
            let bal = balance::current_balance(&state.pool, &account.id).unwrap_or(0);
            return Err((
                StatusCode::PAYMENT_REQUIRED,
                Json(ApiError {
                    error: "insufficient balance".into(),
                    balance_cents: Some(bal),
                    estimated_cost_cents: Some(est_cost),
                    reason: Some("insufficient_balance".into()),
                    reload_url: Some(format!("{}/reload", state.config.public_url)),
                    ..Default::default()
                }),
            ));
        }
    }

    let key_candidates = state.config.upstream.key_candidates(
        provider,
        &format!("embed:{}:{provider}:{model}", req.request_id),
    );
    if key_candidates.is_empty() {
        let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: "upstream embed provider is not configured".into(),
                reason: Some("upstream_not_configured".into()),
                ..Default::default()
            }),
        ));
    }

    let comp = loop {
        let selected_key = match state
            .provider_health
            .choose_key(provider, model, &key_candidates)
            .await
        {
            Ok(key) => key,
            Err(denied) => {
                return Err(release_and_capacity_error(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    denied.reason,
                    denied.retry_after_secs,
                ));
            }
        };

        if let Err(denied) = state
            .rate_limiters
            .check_provider_embed(provider, model)
            .await
        {
            return Err(release_and_capacity_error(
                &state.pool,
                &account.id,
                &req.request_id,
                denied.reason,
                denied.retry_after_secs,
            ));
        }

        match routing::embed_batch_with_key(&selected_key.secret, provider, model, &req.inputs)
            .await
        {
            Ok(c) => break c,
            Err(e) => {
                if let Some(retry_after_secs) = routing::upstream_retry_after(&e) {
                    let cooldown_secs = state
                        .provider_health
                        .record_cooldown(
                            provider,
                            model,
                            &selected_key.fingerprint,
                            retry_after_secs,
                        )
                        .await;
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        provider = %provider,
                        model = %model,
                        key_fingerprint = %selected_key.fingerprint,
                        retry_after_secs = cooldown_secs,
                        error = %e,
                        "embed upstream capacity response; cooled key and retrying"
                    );
                    continue;
                }
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    provider = %provider,
                    model = %model,
                    error = %e,
                    "embed dispatch failed"
                );
                let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
                return Err((
                    StatusCode::BAD_GATEWAY,
                    Json(ApiError {
                        error: "upstream embed error; please retry".into(),
                        reason: Some("upstream_error".into()),
                        ..Default::default()
                    }),
                ));
            }
        }
    };
    if comp.vectors.len() != req.inputs.len() {
        let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ApiError {
                error: format!(
                    "upstream embed returned {} vectors for {} inputs",
                    comp.vectors.len(),
                    req.inputs.len()
                ),
                reason: Some("upstream_error".into()),
                ..Default::default()
            }),
        ));
    }

    // Cost (no output tokens for embeddings).
    let (bluey_cost, customer_cost) = pricing::compute_cost(pricing_entry, comp.input_tokens, 0);
    let charged_customer_cost = if on_trial { 0 } else { customer_cost };

    // Charge.
    let trial_remaining = if on_trial {
        let trial_ms = (((comp.input_tokens.max(1) + 999) / 1000).max(1)) * 1000;
        balance::consume_trial_seconds(&state.pool, &account.id, trial_ms).map_err(|e| {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("trial: {e}"),
                    ..Default::default()
                }),
            )
        })?
    } else {
        let ok = balance::deduct_for_request(
            &state.pool,
            &account.id,
            customer_cost,
            "embed",
            &req.request_id,
        )
        .map_err(|e| {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("deduct: {e}"),
                    ..Default::default()
                }),
            )
        })?;
        if !ok {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                cost_cents = customer_cost,
                "embed post-completion deduct failed; bluey absorbs overrun"
            );
        }
        account.trial_seconds_remaining
    };

    let balance_after = balance::current_balance(&state.pool, &account.id).unwrap_or(0);

    // Auto top-up trigger (same as /router/complete).
    if !on_trial {
        crate::billing::topup::maybe_spawn(
            state.pool.clone(),
            state.config.clone(),
            account.id.clone(),
            balance_after,
            account.auto_topup_enabled,
            account.auto_topup_threshold_cents,
            account.stripe_customer_id.clone(),
            account.stripe_payment_method_id.clone(),
            account.square_customer_id.clone(),
            account.square_card_id.clone(),
            account.auto_topup_amount_cents,
        );
    }

    // Usage event.
    let event = UsageEvent {
        request_id: req.request_id.clone(),
        kind: "embed".into(),
        task_type: None,
        lane: None,
        provider: Some(comp.provider.clone()),
        model: Some(comp.model.clone()),
        input_tokens: comp.input_tokens,
        output_tokens: 0,
        latency_ms: 0,
        cost_cents_to_bluey: bluey_cost,
        cost_cents_to_customer: charged_customer_cost,
        was_speculative: false,
        was_fallback: false,
    };
    if let Err(e) = crate::db::usage::record(&state.pool, &account.id, &event) {
        tracing::warn!(trace_id = %trace_id, error = %e, "failed to record embed usage event");
    }

    let response = EmbedBatchResponse {
        vectors: comp.vectors,
        provider: comp.provider,
        model: comp.model,
        input_tokens: comp.input_tokens,
        cost_cents: charged_customer_cost,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_remaining,
    };

    if let Ok(json) = serde_json::to_string(&response) {
        if let Err(e) = idempotency::mark_complete(&state.pool, &account.id, &req.request_id, &json)
        {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                error = %e,
                "embed mark_complete failed AFTER customer billed"
            );
        }
    }

    Ok(response)
}
