use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use super::{
    billing_restricted_error, capacity_error, missing_provider_key_error,
    priced_transcribe_routes_for, release_and_capacity_error,
    release_and_upstream_spend_guard_check, ApiError, AppState, PricedTranscribeRoute,
};
use crate::auth::AuthedAccount;
use crate::db::{balance, idempotency, usage::UsageEvent};
use crate::{pricing, routing};

#[derive(Deserialize)]
pub struct TranscribeQuery {
    pub request_id: String,
    /// Optional Deepgram model override. Defaults to nova-3. The managed server
    /// still keeps OpenAI gpt-4o-mini-transcribe as the cloud fallback.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TranscribeResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub duration_seconds: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
}

pub async fn transcribe(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    axum::extract::Query(q): axum::extract::Query<TranscribeQuery>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<TranscribeResponse>, (StatusCode, Json<ApiError>)> {
    if let Some(err) = billing_restricted_error(&account) {
        return Err(err);
    }
    if q.request_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "request_id query param is required".into(),
                reason: Some("missing_request_id".into()),
                ..Default::default()
            }),
        ));
    }
    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "audio body is empty".into(),
                reason: Some("empty_audio".into()),
                ..Default::default()
            }),
        ));
    }

    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/wav")
        .to_string();

    match idempotency::reserve(&state.pool, &account.id, &q.request_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("idempotency: {e}"),
                ..Default::default()
            }),
        )
    })? {
        idempotency::ReserveOutcome::FreshReservation => {}
        idempotency::ReserveOutcome::CachedComplete(json) => {
            let cached: TranscribeResponse = serde_json::from_str(&json).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("idempotency cache decode: {e}"),
                        ..Default::default()
                    }),
                )
            })?;
            return Ok(Json(cached));
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

    if let Err(denied) = state.rate_limiters.check_account_stt(&account.id).await {
        return Err(release_and_capacity_error(
            &state.pool,
            &account.id,
            &q.request_id,
            denied.reason,
            denied.retry_after_secs,
        ));
    }
    let on_trial = account.trial_seconds_remaining > 0;
    // Estimate ~1s per ~16KB of audio (rough). Real cost from upstream metadata.
    let est_seconds = (body.len() as i64 / 16_000).max(1);
    let routes = priced_transcribe_routes_for(q.model.as_deref(), est_seconds);
    if routes.is_empty() {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &q.request_id);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "no priced transcribe route available".into(),
                ..Default::default()
            }),
        ));
    }
    let est_cost = routes
        .iter()
        .map(|route| route.estimated_cost_cents)
        .max()
        .unwrap_or(1);
    let est_bluey_cost = routes
        .iter()
        .map(|route| route.estimated_bluey_cost_cents)
        .max()
        .unwrap_or(1);
    if let Some(err) = release_and_upstream_spend_guard_check(
        &state,
        &account.id,
        &q.request_id,
        est_bluey_cost,
        "stt",
    ) {
        return Err(err);
    }
    if !on_trial {
        let can = balance::can_afford(&state.pool, &account.id, est_cost).map_err(|e| {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &q.request_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("balance: {e}"),
                    ..Default::default()
                }),
            )
        })?;
        if !can {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &q.request_id);
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

    let mut last_error: Option<anyhow::Error> = None;
    let mut last_capacity: Option<crate::rate_limit::CapacityDenied> = None;
    let mut last_failure_was_capacity = false;
    let mut selected_route_idx = 0usize;
    let mut selected_route: Option<&PricedTranscribeRoute> = None;
    let mut selected_completion: Option<routing::TranscribeCompletion> = None;

    for (idx, route) in routes.iter().enumerate() {
        let key_candidates = state.config.upstream.key_candidates(
            route.provider,
            &format!("stt:{}:{}:{}", q.request_id, route.provider, route.model),
        );
        if key_candidates.is_empty() {
            last_error = Some(missing_provider_key_error(route.provider));
            continue;
        }

        loop {
            let selected_key = match state
                .provider_health
                .choose_key(route.provider, &route.model, &key_candidates)
                .await
            {
                Ok(key) => key,
                Err(denied) => {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %q.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        retry_after_secs = denied.retry_after_secs,
                        reason = denied.reason,
                        "provider STT key pool cooling down; trying next route"
                    );
                    last_capacity = Some(denied);
                    last_failure_was_capacity = true;
                    break;
                }
            };

            if let Err(denied) = state
                .rate_limiters
                .check_provider_stt(route.provider, &route.model)
                .await
            {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %q.request_id,
                    provider = %route.provider,
                    model = %route.model,
                    retry_after_secs = denied.retry_after_secs,
                    reason = denied.reason,
                    "provider STT capacity busy; trying next route"
                );
                last_capacity = Some(denied);
                last_failure_was_capacity = true;
                break;
            }

            match routing::transcribe_with_key(
                &selected_key.secret,
                route.provider,
                &route.model,
                &body,
                &content_type,
            )
            .await
            {
                Ok(c) => {
                    selected_route_idx = idx;
                    selected_route = Some(route);
                    selected_completion = Some(c);
                    break;
                }
                Err(e) => {
                    if let Some(retry_after_secs) = routing::upstream_retry_after(&e) {
                        let cooldown_secs = state
                            .provider_health
                            .record_cooldown(
                                route.provider,
                                &route.model,
                                &selected_key.fingerprint,
                                retry_after_secs,
                            )
                            .await;
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %q.request_id,
                            provider = %route.provider,
                            model = %route.model,
                            key_fingerprint = %selected_key.fingerprint,
                            retry_after_secs = cooldown_secs,
                            error = %e,
                            "transcribe upstream capacity response; cooled key and retrying route"
                        );
                        last_capacity = Some(crate::rate_limit::CapacityDenied {
                            retry_after_secs: cooldown_secs,
                            reason: "provider_key_cooling_down",
                        });
                        last_failure_was_capacity = true;
                        continue;
                    }
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %q.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        error = %e,
                        "transcribe dispatch failed; trying next route"
                    );
                    last_error = Some(e);
                    last_failure_was_capacity = false;
                    break;
                }
            }
        }

        if selected_completion.is_some() {
            break;
        }
    }

    let (selected_route, comp) = match (selected_route, selected_completion) {
        (Some(route), Some(completion)) => (route, completion),
        _ => {
            let _ = idempotency::release(&state.pool, &account.id, &q.request_id);
            if last_failure_was_capacity {
                let denied = last_capacity.expect("capacity flag set with no capacity denial");
                return Err(capacity_error(denied.reason, denied.retry_after_secs));
            }
            if let Some(e) = last_error {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %q.request_id,
                    error = %e,
                    "all transcribe routes failed"
                );
            }
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "upstream transcribe error; please retry".into(),
                    reason: Some("upstream_error".into()),
                    ..Default::default()
                }),
            ));
        }
    };

    // Cost billed against duration_seconds as input "tokens".
    let (bluey_cost, customer_cost) =
        pricing::compute_cost(selected_route.pricing, comp.duration_seconds, 0);
    let charged_customer_cost = if on_trial { 0 } else { customer_cost };

    if on_trial {
        let trial_ms = comp.duration_seconds.max(1) * 1000;
        if let Err(e) = balance::consume_trial_seconds(&state.pool, &account.id, trial_ms) {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &q.request_id);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("trial: {e}"),
                    ..Default::default()
                }),
            ));
        }
    } else {
        let ok = balance::deduct_for_request(
            &state.pool,
            &account.id,
            customer_cost,
            "transcribe",
            &q.request_id,
        )
        .map_err(|e| {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &q.request_id);
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
                "transcribe post-completion deduct failed; bluey absorbs overrun"
            );
        }
    }

    let balance_after = balance::current_balance(&state.pool, &account.id).unwrap_or(0);

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

    let event = UsageEvent {
        request_id: q.request_id.clone(),
        kind: "stt".into(),
        task_type: None,
        lane: None,
        provider: Some(comp.provider.clone()),
        model: Some(comp.model.clone()),
        input_tokens: comp.duration_seconds,
        output_tokens: 0,
        latency_ms: 0,
        cost_cents_to_bluey: bluey_cost,
        cost_cents_to_customer: charged_customer_cost,
        was_speculative: false,
        was_fallback: selected_route_idx > 0,
    };
    if let Err(e) = crate::db::usage::record(&state.pool, &account.id, &event) {
        tracing::warn!(trace_id = %trace_id, error = %e, "failed to record transcribe usage event");
    }

    let response = TranscribeResponse {
        text: comp.text,
        provider: comp.provider,
        model: comp.model,
        duration_seconds: comp.duration_seconds,
        cost_cents: charged_customer_cost,
        balance_cents_after: balance_after,
    };

    if let Ok(json) = serde_json::to_string(&response) {
        if let Err(e) = idempotency::mark_complete(&state.pool, &account.id, &q.request_id, &json) {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %q.request_id,
                error = %e,
                "transcribe mark_complete failed AFTER customer billed"
            );
        }
    }

    Ok(Json(response))
}
