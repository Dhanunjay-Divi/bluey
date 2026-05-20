//! Real router endpoints: managed dispatch + atomic deduction + idempotency.

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Extension, Json,
};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Instant;

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::accounts::Account;
use crate::db::{balance, idempotency, usage::UsageEvent};
use crate::pricing;
use crate::routing;

#[derive(Deserialize)]
pub struct CompleteRequest {
    /// Client-supplied idempotency key. REQUIRED. Codex S4.1: a retry
    /// after a network timeout/lost response must not be charged twice.
    /// The server rejects duplicate (account_id, request_id) pairs by
    /// returning the cached response (200 if completed) or 409 (if the
    /// original is still in flight).
    pub request_id: String,
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

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Default)]
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
    Ok(Json(complete_inner(state, account, req).await?))
}

/// Streaming variant of `/router/complete`.
///
/// The v0.2 managed billing invariant is "dispatch once, charge once, cache
/// once". To preserve that invariant, this handler runs the same atomic
/// `complete_inner` lifecycle and then emits the final answer as small
/// OpenAI-compatible SSE deltas plus one `billing` event. Future stages can
/// swap the internals to true upstream streaming as long as the same
/// idempotency/deduction contract remains intact.
pub async fn complete_stream(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<CompleteRequest>,
) -> Result<
    Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ApiError>),
> {
    let response = complete_inner(state, account, req).await?;
    let events = response_to_sse_events(response);
    Ok(Sse::new(stream::iter(events.into_iter().map(Ok))))
}

async fn complete_inner(
    state: AppState,
    account: Account,
    req: CompleteRequest,
) -> Result<CompleteResponse, (StatusCode, Json<ApiError>)> {
    // 0. Validate request_id is non-empty.
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

    // Codex S4.4: managed dispatcher does not run local models.
    // The daemon's LocalFallbackPolicy must dispatch local-lane work
    // directly to on-device Ollama; the managed cloud path is not the
    // right home for it.
    if req.lane == "local" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "local lane is daemon-only; managed cloud does not run local models".into(),
                reason: Some("local_lane_unsupported".into()),
                ..Default::default()
            }),
        ));
    }

    // 1. Idempotency check + reservation. Codex S4.1.
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
            // Replay: return the cached terminal response.
            let cached: CompleteResponse = serde_json::from_str(&json).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("idempotency cache decode: {e}"),
                        ..Default::default()
                    }),
                )
            })?;
            return Ok(cached);
        }
        idempotency::ReserveOutcome::InProgress => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "request already in progress; wait for original to complete".into(),
                    reason: Some("request_in_progress".into()),
                    ..Default::default()
                }),
            ));
        }
        idempotency::ReserveOutcome::CachedFailed => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "previous attempt with this request_id failed; use a new request_id"
                        .into(),
                    reason: Some("request_failed_terminal".into()),
                    ..Default::default()
                }),
            ));
        }
    }

    // 2. Resolve lane → provider+model.
    let (provider, model) = routing::resolve_route(&req.lane);
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

    // 3. Estimate cost ceiling for the entry check.
    let max_out = req.max_tokens.unwrap_or(2048) as i64;
    let est_in = req.estimated_input_tokens.unwrap_or_else(|| {
        // Crude fallback: ~4 chars/token
        ((req.system.len() + req.user.len()) as i64) / 4
    });
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_in, max_out);

    let on_trial = account.trial_seconds_remaining > 0;

    // 4. Entry check — unless the account is still on the free trial.
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
                }),
            ));
        }
    }

    // 5. Dispatch to upstream provider. Pass the entry estimate so the
    //    dispatcher can fall back to it if the upstream omits `usage`.
    //    Codex S4.5.
    let started = Instant::now();
    let result = routing::complete(
        &state.config.upstream,
        provider,
        model,
        &req.system,
        &req.user,
        req.max_tokens,
        req.temperature,
        Some(est_in),
    )
    .await;
    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis() as i64;

    let comp = match result {
        Ok(c) => c,
        Err(e) => {
            // Codex S4.6: log raw upstream details, return sanitized
            // message to the customer. We DO NOT mark the idempotency
            // row as failed-terminal because a transient upstream error
            // should be retryable with the same request_id (the
            // alternative — making the customer mint a new id — is
            // user-hostile for ephemeral 503s).
            tracing::warn!(
                account_id = %account.id,
                request_id = %req.request_id,
                provider = %provider,
                model = %model,
                error = %e,
                "upstream dispatch failed"
            );
            let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    error: "upstream provider error; please retry".into(),
                    reason: Some("upstream_error".into()),
                    ..Default::default()
                }),
            ));
        }
    };

    // 6. Compute actual cost from real token counts.
    let (bluey_cost, customer_cost) =
        pricing::compute_cost(pricing_entry, comp.input_tokens, comp.output_tokens);

    // 7. Charge: trial decrement OR balance deduction.
    let trial_remaining = if on_trial {
        balance::consume_trial_seconds(&state.pool, &account.id, elapsed_ms).map_err(|e| {
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
        let ok = balance::deduct(&state.pool, &account.id, customer_cost).map_err(|e| {
            let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: format!("deduct: {e}"),
                    ..Default::default()
                }),
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

    // Codex Stage 10: auto top-up trigger. Fire-and-forget; the actual
    // charge resolves on the executor and the webhook for the
    // resulting payment_intent.succeeded credits the balance via the
    // existing /billing/webhook flow. This closes the v0.2 dealbreaker
    // gap: customers no longer hit hard-stop without an obvious recovery.
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
            account.auto_topup_amount_cents,
        );
    }

    // 8. Record usage event. Reuse the client-supplied request_id so
    //    Stage 7's idempotent ingest dedupes correctly across retries.
    let event = UsageEvent {
        request_id: req.request_id.clone(),
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

    let response = CompleteResponse {
        text: comp.text,
        provider: comp.provider,
        model: comp.model,
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        cost_cents: customer_cost,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_remaining,
    };

    // 9. Cache the terminal response in the idempotency row so a retry
    //    returns this exact body without re-dispatching.
    // Codex Stage 9c (S4 round-2 nit): mark_complete failure must NOT
    // be silently dropped. The customer has been billed and the upstream
    // call has finished; if we cannot persist the cached response, a
    // retry hits the in_progress reservation and 409s the customer
    // permanently. Log at error with the request_id so SREs can
    // reconcile manually. A future stage adds a Prometheus counter at
    // /admin/metrics.
    match serde_json::to_string(&response) {
        Ok(json) => {
            if let Err(e) =
                idempotency::mark_complete(&state.pool, &account.id, &req.request_id, &json)
            {
                tracing::error!(
                    account_id = %account.id,
                    request_id = %req.request_id,
                    error = %e,
                    "idempotency::mark_complete failed AFTER customer billed; retry will return 409 — manual reconciliation required"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                account_id = %account.id,
                request_id = %req.request_id,
                error = %e,
                "failed to serialize response for idempotency cache; retry will return 409 — manual reconciliation required"
            );
        }
    }

    Ok(response)
}

fn response_to_sse_events(response: CompleteResponse) -> Vec<Event> {
    let mut events = response
        .text
        .split_inclusive(char::is_whitespace)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| {
            Event::default().data(
                serde_json::json!({
                    "choices": [
                        { "delta": { "content": chunk } }
                    ]
                })
                .to_string(),
            )
        })
        .collect::<Vec<_>>();
    if events.is_empty() {
        events.push(
            Event::default().data(
                serde_json::json!({
                    "choices": [
                        { "delta": { "content": "" } }
                    ]
                })
                .to_string(),
            ),
        );
    }
    let billing = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
    events.push(Event::default().event("billing").data(billing));
    events.push(Event::default().data("[DONE]"));
    events
}

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
}

pub async fn embed(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<EmbedRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, Json<ApiError>)> {
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
            let cached: EmbedResponse = serde_json::from_str(&json).map_err(|e| {
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

    // Entry check (skipped on trial).
    let on_trial = account.trial_seconds_remaining > 0;
    let est_in = (req.input.len() as i64) / 4;
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_in, 0);
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
                }),
            ));
        }
    }

    // Dispatch.
    let result = routing::embed(&state.config.upstream, provider, model, &req.input).await;

    let comp = match result {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                account_id = %account.id,
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
    };

    // Cost (no output tokens for embeddings).
    let (bluey_cost, customer_cost) = pricing::compute_cost(pricing_entry, comp.input_tokens, 0);

    // Charge.
    if !on_trial {
        let ok = balance::deduct(&state.pool, &account.id, customer_cost).map_err(|e| {
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
                account_id = %account.id,
                cost_cents = customer_cost,
                "embed post-completion deduct failed; bluey absorbs overrun"
            );
        }
    }

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
        cost_cents_to_customer: customer_cost,
        was_speculative: false,
        was_fallback: false,
    };
    if let Err(e) = crate::db::usage::record(&state.pool, &account.id, &event) {
        tracing::warn!(error = %e, "failed to record embed usage event");
    }

    let response = EmbedResponse {
        vector: comp.vector,
        provider: comp.provider,
        model: comp.model,
        input_tokens: comp.input_tokens,
        cost_cents: customer_cost,
        balance_cents_after: balance_after,
    };

    if let Ok(json) = serde_json::to_string(&response) {
        if let Err(e) = idempotency::mark_complete(&state.pool, &account.id, &req.request_id, &json)
        {
            tracing::error!(
                account_id = %account.id,
                request_id = %req.request_id,
                error = %e,
                "embed mark_complete failed AFTER customer billed"
            );
        }
    }

    Ok(Json(response))
}

#[derive(Deserialize)]
pub struct TranscribeQuery {
    pub request_id: String,
    /// Optional Deepgram model override. Defaults to nova-3.
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
    axum::extract::Query(q): axum::extract::Query<TranscribeQuery>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<TranscribeResponse>, (StatusCode, Json<ApiError>)> {
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

    let provider = "deepgram";
    let model = q.model.as_deref().unwrap_or("nova-3");
    let pricing_entry = pricing::lookup(provider, model).ok_or_else(|| {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &q.request_id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("no pricing for {provider}/{model}"),
                ..Default::default()
            }),
        )
    })?;

    let on_trial = account.trial_seconds_remaining > 0;
    // Estimate ~1s per ~16KB of audio (rough). Real cost from upstream metadata.
    let est_seconds = (body.len() as i64 / 16_000).max(1);
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_seconds, 0);
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
                }),
            ));
        }
    }

    let result = routing::transcribe(
        &state.config.upstream,
        provider,
        model,
        &body,
        &content_type,
    )
    .await;

    let comp = match result {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                account_id = %account.id,
                request_id = %q.request_id,
                provider = %provider,
                model = %model,
                error = %e,
                "transcribe dispatch failed"
            );
            let _ = idempotency::release(&state.pool, &account.id, &q.request_id);
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
        pricing::compute_cost(pricing_entry, comp.duration_seconds, 0);

    if !on_trial {
        let ok = balance::deduct(&state.pool, &account.id, customer_cost).map_err(|e| {
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
                account_id = %account.id,
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
        cost_cents_to_customer: customer_cost,
        was_speculative: false,
        was_fallback: false,
    };
    if let Err(e) = crate::db::usage::record(&state.pool, &account.id, &event) {
        tracing::warn!(error = %e, "failed to record transcribe usage event");
    }

    let response = TranscribeResponse {
        text: comp.text,
        provider: comp.provider,
        model: comp.model,
        duration_seconds: comp.duration_seconds,
        cost_cents: customer_cost,
        balance_cents_after: balance_after,
    };

    if let Ok(json) = serde_json::to_string(&response) {
        if let Err(e) = idempotency::mark_complete(&state.pool, &account.id, &q.request_id, &json) {
            tracing::error!(
                account_id = %account.id,
                request_id = %q.request_id,
                error = %e,
                "transcribe mark_complete failed AFTER customer billed"
            );
        }
    }

    Ok(Json(response))
}
