use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use super::{
    billing_restricted_error, capacity_error, missing_provider_key_error,
    priced_transcribe_routes_for, prior_provider_exposure_error, provider_accounting_pending_error,
    provider_cost_guard, release_and_capacity_error, settle_provider_attempt_before_customer,
    ApiError, AppState, PricedTranscribeRoute,
};
use crate::auth::AuthedAccount;
use crate::db::{
    balance, idempotency,
    usage::UsageEvent,
    usage_reservations::{self, ReserveUsageInput, SettlementUsageEvent},
};
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
    if let Some(err) = prior_provider_exposure_error(
        &state,
        &account.id,
        &q.request_id,
        &format!("router:{}:stt", q.request_id),
    ) {
        return Err(err);
    }
    let created_at_ms = chrono::Utc::now().timestamp_millis();
    let usage_reservation = usage_reservations::reserve(
        &state.pool,
        ReserveUsageInput {
            account_id: &account.id,
            request_id: &q.request_id,
            kind: "stt",
            reason: "transcribe",
            estimated_customer_cents: est_cost,
            estimated_upstream_cents: 0,
            upstream_spend_guard: state.config.upstream_spend_guard,
            created_at_ms,
            expires_at_ms: created_at_ms.saturating_add(30 * 60 * 1_000),
        },
    )
    .map_err(|error| {
        let _ = idempotency::release(&state.pool, &account.id, &q.request_id);
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
                    error: format!("transcribe usage reservation failed: {error}"),
                    reason: Some("usage_reservation_failed".into()),
                    ..Default::default()
                }),
            ),
        }
    })?;
    let on_trial = usage_reservation.is_trial();

    let mut last_error: Option<anyhow::Error> = None;
    let mut last_capacity: Option<crate::rate_limit::CapacityDenied> = None;
    let mut last_failure_was_capacity = false;
    let mut selected_route_idx = 0usize;
    let mut selected_route: Option<&PricedTranscribeRoute> = None;
    let mut selected_completion: Option<routing::TranscribeCompletion> = None;
    let mut dispatch_index = 0_usize;

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

            let attempt_request_id = format!("{}:stt-attempt:{dispatch_index}", q.request_id);
            dispatch_index = dispatch_index.saturating_add(1);
            let mut attempt_guard = match provider_cost_guard::reserve(
                &state.pool,
                state.config.upstream_spend_guard,
                &account.id,
                &format!("router:{}:stt", q.request_id),
                &attempt_request_id,
                route.provider,
                &route.model,
                route.estimated_bluey_cost_cents,
                "stt_attempt",
                "stt",
            ) {
                Ok(provider_cost_guard::Admission::Held(guard)) => guard,
                Ok(provider_cost_guard::Admission::Unconfigured) => {
                    last_error = Some(anyhow::anyhow!(
                        "paid STT route unexpectedly had zero projected exposure"
                    ));
                    break;
                }
                Ok(provider_cost_guard::Admission::GlobalLimit) | Err(_) => {
                    last_error = Some(anyhow::anyhow!("upstream spend guard denied STT route"));
                    break;
                }
            };

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
                    let route_matches = c.provider == route.provider && c.model == route.model;
                    let actual_bluey_cost = pricing::lookup(&c.provider, &c.model)
                        .map(|price| pricing::compute_cost(price, c.duration_seconds, 0).0)
                        .unwrap_or(crate::db::usage::MAX_AUTHORITATIVE_EVENT_COST_CENTS);
                    let attempt_event = UsageEvent {
                        request_id: attempt_request_id,
                        kind: "stt_attempt".into(),
                        task_type: Some("stt".into()),
                        lane: None,
                        provider: Some(c.provider.clone()),
                        model: Some(c.model.clone()),
                        input_tokens: c.duration_seconds,
                        output_tokens: 0,
                        latency_ms: 0,
                        cost_cents_to_bluey: actual_bluey_cost,
                        cost_cents_to_customer: 0,
                        was_speculative: false,
                        was_fallback: idx > 0,
                    };
                    settle_provider_attempt_before_customer(
                        &state.pool,
                        &account.id,
                        &q.request_id,
                        &mut attempt_guard,
                        attempt_event,
                        actual_bluey_cost,
                    )?;
                    if !route_matches {
                        last_error = Some(anyhow::anyhow!("STT provider route identity mismatch"));
                        last_failure_was_capacity = false;
                        break;
                    }
                    selected_route_idx = idx;
                    selected_route = Some(route);
                    selected_completion = Some(c);
                    break;
                }
                Err(e) => {
                    if let Err(error) = attempt_guard.settle_conservative() {
                        tracing::error!(request_id = %q.request_id, error = %error, "STT failed-attempt settlement pending reconciliation");
                        return Err(provider_accounting_pending_error(
                            &state.pool,
                            &account.id,
                            &q.request_id,
                        ));
                    }
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
            let _ = usage_reservations::release(
                &state.pool,
                &account.id,
                &q.request_id,
                "provider_dispatch_failed",
                chrono::Utc::now().timestamp_millis(),
            );
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
    let (_bluey_cost, customer_cost) =
        pricing::compute_cost(selected_route.pricing, comp.duration_seconds, 0);
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
        cost_cents_to_bluey: 0,
        cost_cents_to_customer: customer_cost,
        was_speculative: false,
        was_fallback: selected_route_idx > 0,
    };
    let settled = usage_reservations::settle_with_events(
        &state.pool,
        &account.id,
        &q.request_id,
        customer_cost,
        comp.duration_seconds.max(1).saturating_mul(1_000),
        "completed",
        chrono::Utc::now().timestamp_millis(),
        &[SettlementUsageEvent {
            event,
            customer_cost_cents: customer_cost,
        }],
    )
    .map_err(|error| {
        tracing::error!(trace_id = %trace_id, error = %error, "transcribe usage settlement pending");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "transcribe usage settlement is pending reconciliation".into(),
                reason: Some("usage_settlement_pending".into()),
                ..Default::default()
            }),
        )
    })?;
    let charged_customer_cost = settled.charged_customer_cents;
    let balance_after = settled.balance_cents_after;

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
