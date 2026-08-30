use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use super::{
    billing_restricted_error, prior_provider_exposure_error, provider_accounting_pending_error,
    provider_cost_guard, release_and_capacity_error, settle_provider_attempt_before_customer,
    spawn_usage_expiry_reconciler, ApiError, AppState,
};
use crate::auth::AuthedAccount;
use crate::db::{
    balance, idempotency,
    usage::UsageEvent,
    usage_reservations::{self, ReserveUsageInput, SettlementUsageEvent},
};
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
async fn embed_batch_inner(
    state: &AppState,
    account: &crate::db::accounts::Account,
    _trace_id: &str,
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
    // Entry estimate. Customer funds/trial are atomically reserved below,
    // after provider configuration is verified and before network dispatch.
    let est_in = pricing::utf8_input_token_upper_bound(req.inputs.iter().map(String::as_str));
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_in, 0);
    let est_bluey_cost = pricing::estimate_bluey_cost_ceiling(pricing_entry, est_in, 0);
    if let Some(err) = prior_provider_exposure_error(
        state,
        &account.id,
        &req.request_id,
        &format!("router:{}:embed", req.request_id),
    ) {
        return Err(err);
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

    let created_at_ms = chrono::Utc::now().timestamp_millis();
    let usage_reservation = usage_reservations::reserve(
        &state.pool,
        ReserveUsageInput {
            account_id: &account.id,
            request_id: &req.request_id,
            kind: "embed",
            reason: "embed",
            estimated_customer_cents: est_cost,
            estimated_upstream_cents: 0,
            upstream_spend_guard: state.config.upstream_spend_guard,
            created_at_ms,
            expires_at_ms: created_at_ms.saturating_add(30 * 60 * 1_000),
        },
    )
    .map_err(|error| {
        let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
        match error {
            usage_reservations::UsageReservationError::InsufficientBalance => (
                StatusCode::PAYMENT_REQUIRED,
                Json(ApiError {
                    error: "insufficient balance".into(),
                    balance_cents: balance::current_balance(&state.pool, &account.id).ok(),
                    estimated_cost_cents: Some(est_cost),
                    reason: Some("insufficient_balance".into()),
                    reload_url: Some(format!("{}/reload", state.config.public_url)),
                    ..Default::default()
                }),
            ),
            _ => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiError {
                    error: format!("embed usage reservation failed: {error}"),
                    reason: Some("usage_reservation_failed".into()),
                    ..Default::default()
                }),
            ),
        }
    })?;
    spawn_usage_expiry_reconciler(
        state.pool.clone(),
        account.id.clone(),
        req.request_id.clone(),
        usage_reservation.attempt,
        usage_reservation.expires_at_ms,
    );
    let on_trial = usage_reservation.is_trial();

    let mut dispatch_index = 0_usize;
    let comp = loop {
        let selected_key = match state
            .provider_health
            .choose_key(provider, model, &key_candidates)
            .await
        {
            Ok(key) => key,
            Err(denied) => {
                let _ = usage_reservations::release(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    "provider_capacity",
                    chrono::Utc::now().timestamp_millis(),
                );
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
            let _ = usage_reservations::release(
                &state.pool,
                &account.id,
                &req.request_id,
                "provider_capacity",
                chrono::Utc::now().timestamp_millis(),
            );
            return Err(release_and_capacity_error(
                &state.pool,
                &account.id,
                &req.request_id,
                denied.reason,
                denied.retry_after_secs,
            ));
        }

        let attempt_request_id = format!("{}:embed-attempt:{dispatch_index}", req.request_id);
        dispatch_index = dispatch_index.saturating_add(1);
        let mut attempt_guard = match provider_cost_guard::reserve(
            &state.pool,
            state.config.upstream_spend_guard,
            &account.id,
            &format!("router:{}:embed", req.request_id),
            &attempt_request_id,
            provider,
            model,
            est_bluey_cost,
            "embed_attempt",
            "embed",
        ) {
            Ok(provider_cost_guard::Admission::Held(guard)) => guard,
            Ok(provider_cost_guard::Admission::Unconfigured) => {
                let _ = usage_reservations::release(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    "upstream_spend_guard",
                    chrono::Utc::now().timestamp_millis(),
                );
                let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError {
                        error: "embed pricing produced no durable upstream exposure".into(),
                        reason: Some("upstream_spend_guard".into()),
                        ..Default::default()
                    }),
                ));
            }
            Ok(provider_cost_guard::Admission::GlobalLimit) | Err(_) => {
                let _ = usage_reservations::release(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    "upstream_spend_guard",
                    chrono::Utc::now().timestamp_millis(),
                );
                let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ApiError {
                        error: "upstream spend guard paused embedding dispatch".into(),
                        reason: Some("upstream_spend_guard".into()),
                        ..Default::default()
                    }),
                ));
            }
        };

        match routing::embed_batch_with_key(&selected_key.secret, provider, model, &req.inputs)
            .await
        {
            Ok(c) => {
                let route_matches = c.provider == provider && c.model == model;
                let actual_bluey_cost = pricing::lookup(&c.provider, &c.model)
                    .map(|price| pricing::compute_cost(price, c.input_tokens, 0).0)
                    .unwrap_or(crate::db::usage::MAX_AUTHORITATIVE_EVENT_COST_CENTS);
                let attempt_event = UsageEvent {
                    request_id: attempt_request_id,
                    kind: "embed_attempt".into(),
                    task_type: Some("embed".into()),
                    lane: None,
                    provider: Some(c.provider.clone()),
                    model: Some(c.model.clone()),
                    input_tokens: c.input_tokens,
                    output_tokens: 0,
                    latency_ms: 0,
                    cost_cents_to_bluey: actual_bluey_cost,
                    cost_cents_to_customer: 0,
                    was_speculative: false,
                    was_fallback: dispatch_index > 1,
                };
                settle_provider_attempt_before_customer(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    &mut attempt_guard,
                    attempt_event,
                    actual_bluey_cost,
                    c.usage_provenance,
                )?;
                if !route_matches {
                    let _ = usage_reservations::release(
                        &state.pool,
                        &account.id,
                        &req.request_id,
                        "upstream_route_mismatch",
                        chrono::Utc::now().timestamp_millis(),
                    );
                    let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ApiError {
                            error: "embedding provider route identity mismatch".into(),
                            reason: Some("upstream_route_mismatch".into()),
                            ..Default::default()
                        }),
                    ));
                }
                break c;
            }
            Err(e) => {
                if let Err(error) = attempt_guard.settle_conservative() {
                    tracing::error!(request_id = %req.request_id, error = %error, "embed failed-attempt settlement pending reconciliation");
                    return Err(provider_accounting_pending_error(
                        &state.pool,
                        &account.id,
                        &req.request_id,
                    ));
                }
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
                let _ = usage_reservations::release(
                    &state.pool,
                    &account.id,
                    &req.request_id,
                    "upstream_error",
                    chrono::Utc::now().timestamp_millis(),
                );
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
        let _ = usage_reservations::release(
            &state.pool,
            &account.id,
            &req.request_id,
            "invalid_provider_response",
            chrono::Utc::now().timestamp_millis(),
        );
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
    let (_bluey_cost, customer_cost) = pricing::compute_cost(pricing_entry, comp.input_tokens, 0);
    let trial_ms = (((comp.input_tokens.max(1) + 999) / 1000).max(1)) * 1000;
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
        cost_cents_to_bluey: 0,
        cost_cents_to_customer: customer_cost,
        was_speculative: false,
        was_fallback: false,
    };
    let settled = usage_reservations::settle_with_events(
        &state.pool,
        &account.id,
        &req.request_id,
        customer_cost,
        trial_ms,
        "completed",
        chrono::Utc::now().timestamp_millis(),
        &[SettlementUsageEvent {
            event,
            customer_cost_cents: customer_cost,
        }],
    )
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("embed usage settlement failed: {error}"),
                reason: Some("usage_settlement_pending".into()),
                ..Default::default()
            }),
        )
    })?;
    let charged_customer_cost = settled.charged_customer_cents;
    let trial_remaining = settled.trial_seconds_remaining;
    let balance_after = settled.balance_cents_after;

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
