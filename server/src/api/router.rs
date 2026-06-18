//! Real router endpoints: managed dispatch + atomic deduction + idempotency.

use axum::{
    extract::State,
    http::StatusCode,
    response::sse::{Event, Sse},
    Extension, Json,
};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::pin::Pin;
use std::time::Instant;

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::accounts::Account;
use crate::db::{
    balance, idempotency, sync,
    usage::{self, UsageEvent},
};
use crate::pricing;
use crate::routing;

type RouterSseStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<Event, Infallible>> + Send + 'static>>;

fn log_session_id(session_id: Option<&str>) -> &str {
    session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("none")
}

struct StreamingIdempotencyGuard {
    pool: crate::db::DbPool,
    account_id: String,
    request_id: String,
    drop_action: StreamingDropAction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StreamingDropAction {
    Release,
    KeepInProgress,
    None,
}

impl StreamingIdempotencyGuard {
    fn new(pool: crate::db::DbPool, account_id: String, request_id: String) -> Self {
        Self {
            pool,
            account_id,
            request_id,
            drop_action: StreamingDropAction::Release,
        }
    }

    fn release_now(&mut self) {
        if self.drop_action != StreamingDropAction::Release {
            return;
        }
        if let Err(e) = idempotency::release(&self.pool, &self.account_id, &self.request_id) {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                request_id = %self.request_id,
                error = %e,
                "failed to release streaming idempotency reservation"
            );
        }
        self.drop_action = StreamingDropAction::None;
    }

    fn mark_failed_now(&mut self) {
        if self.drop_action == StreamingDropAction::None {
            return;
        }
        if let Err(e) = idempotency::mark_failed(&self.pool, &self.account_id, &self.request_id) {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                request_id = %self.request_id,
                error = %e,
                "failed to mark streaming idempotency reservation failed"
            );
        }
        self.drop_action = StreamingDropAction::None;
    }

    fn mark_complete_now(&mut self, response_json: &str) -> anyhow::Result<()> {
        let result = idempotency::mark_complete(
            &self.pool,
            &self.account_id,
            &self.request_id,
            response_json,
        );
        self.drop_action = StreamingDropAction::None;
        result
    }

    fn keep_in_progress_for_manual_reconciliation(&mut self) {
        if self.drop_action != StreamingDropAction::None {
            self.drop_action = StreamingDropAction::KeepInProgress;
        }
    }
}

impl Drop for StreamingIdempotencyGuard {
    fn drop(&mut self) {
        match self.drop_action {
            StreamingDropAction::Release => {
                if let Err(e) = idempotency::release(&self.pool, &self.account_id, &self.request_id)
                {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                        request_id = %self.request_id,
                        error = %e,
                        "failed to release dropped streaming idempotency reservation"
                    );
                }
            }
            StreamingDropAction::KeepInProgress => tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&self.account_id),
                request_id = %self.request_id,
                "streaming response was dropped after deltas were delivered; reservation kept in progress for reconciliation"
            ),
            StreamingDropAction::None => {}
        }
    }
}

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
    pub session_id: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Optional provider-neutral reasoning effort. Supported values:
    /// off/low/medium/high/auto. Server policy still decides whether the
    /// selected provider/model can safely apply it.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Optional internal thinking token budget for provider families that
    /// expose it. Clamped server-side.
    #[serde(default)]
    pub thinking_budget_tokens: Option<u32>,
    pub lane: String,
    #[serde(default)]
    pub estimated_input_tokens: Option<i64>,
    /// User-approved screenshot/screen-analysis images as provider-compatible
    /// data URLs. Presence of any image forces the managed lane to `vision`.
    #[serde(default)]
    pub image_data_urls: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
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
    pub retry_after_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reload_url: Option<String>,
}

#[derive(Clone, Copy)]
struct PricedRoute {
    provider: &'static str,
    model: &'static str,
    pricing: pricing::ModelPricing,
    estimated_cost_cents: i64,
    estimated_bluey_cost_cents: i64,
}

struct PricedTranscribeRoute {
    provider: &'static str,
    model: String,
    pricing: &'static pricing::ModelPricing,
    estimated_cost_cents: i64,
    estimated_bluey_cost_cents: i64,
}

fn capacity_error(reason: &str, retry_after_secs: u64) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ApiError {
            error: "Bluey is handling a burst right now; retry shortly".into(),
            reason: Some(reason.to_string()),
            retry_after_secs: Some(retry_after_secs),
            ..Default::default()
        }),
    )
}

fn release_and_capacity_error(
    pool: &crate::db::DbPool,
    account_id: &str,
    request_id: &str,
    reason: &str,
    retry_after_secs: u64,
) -> (StatusCode, Json<ApiError>) {
    let _ = idempotency::release(pool, account_id, request_id);
    capacity_error(reason, retry_after_secs)
}

fn release_and_upstream_spend_guard_check(
    state: &AppState,
    account_id: &str,
    request_id: &str,
    projected_bluey_cents: i64,
    kind: &str,
) -> Option<(StatusCode, Json<ApiError>)> {
    let guard = state.config.upstream_spend_guard?;
    if projected_bluey_cents <= 0 {
        return None;
    }
    let current = match usage::bluey_spend_cents_in_window(&state.pool, guard.window_hours) {
        Ok(value) => value,
        Err(e) => {
            let _ = idempotency::release(&state.pool, account_id, request_id);
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                request_id = %request_id,
                kind,
                error = %e,
                "upstream spend guard query failed"
            );
            return Some((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "upstream spend guard unavailable".into(),
                    reason: Some("upstream_spend_guard_unavailable".into()),
                    ..Default::default()
                }),
            ));
        }
    };
    let projected_total = current.saturating_add(projected_bluey_cents);
    if projected_total > guard.limit_cents {
        let _ = idempotency::release(&state.pool, account_id, request_id);
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            request_id = %request_id,
            kind,
            current_bluey_cents = current,
            projected_bluey_cents,
            limit_bluey_cents = guard.limit_cents,
            window_hours = guard.window_hours,
            "upstream spend guard paused managed dispatch"
        );
        return Some((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                error: "Bluey live-test budget is paused; operator action required".into(),
                reason: Some("upstream_spend_guard".into()),
                retry_after_secs: Some(3600),
                ..Default::default()
            }),
        ));
    }
    None
}

/// First-token deadline for managed streaming. A provider that accepts the
/// request (2xx) but produces no first event within this budget is treated
/// as a stalled candidate and the router falls back to the next route
/// instead of hanging. An in-band error frame or an empty stream still
/// counts as a response and is surfaced through the normal consume path.
/// Override with BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS.
const DEFAULT_FIRST_TOKEN_TIMEOUT_MS: u64 = 6_000;
const DEFAULT_DEEP_FIRST_TOKEN_TIMEOUT_MS: u64 = 30_000;

fn first_token_deadline_for_lane(
    effective_lane: &str,
    has_thinking_budget: bool,
) -> std::time::Duration {
    if effective_lane == "deep" || has_thinking_budget {
        let ms = std::env::var("BLUEY_STREAM_DEEP_FIRST_TOKEN_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|ms| *ms > 0)
            .unwrap_or(DEFAULT_DEEP_FIRST_TOKEN_TIMEOUT_MS);
        return std::time::Duration::from_millis(ms);
    }
    first_token_deadline()
}

/// Time budget for managed cloud RAG retrieval before answering. RAG
/// enrichment is best-effort context, not correctness — it must never
/// delay the first token by more than this. If the lexical/vector query
/// over cloud_rag_chunks exceeds the budget the answer proceeds without
/// retrieved context. Override with BLUEY_RAG_RETRIEVAL_BUDGET_MS.
const DEFAULT_RAG_RETRIEVAL_BUDGET_MS: u64 = 300;

fn rag_retrieval_budget() -> std::time::Duration {
    let ms = std::env::var("BLUEY_RAG_RETRIEVAL_BUDGET_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .unwrap_or(DEFAULT_RAG_RETRIEVAL_BUDGET_MS);
    std::time::Duration::from_millis(ms)
}

/// Budgeted wrapper around `completion_rag_matches`. The underlying query is
/// blocking SQLite, so it runs on the blocking pool; a `timeout` caps how
/// long the request waits. On timeout/join failure the answer proceeds with
/// no retrieved context (the blocking task is allowed to finish and its
/// result dropped — it just no longer holds up the first token).
async fn completion_rag_matches_budgeted(
    pool: &crate::db::DbPool,
    account_id: &str,
    session_id: Option<&str>,
    query: &str,
) -> Vec<sync::RagMatch> {
    if query.trim().chars().count() < 8 {
        return Vec::new();
    }
    let budget = rag_retrieval_budget();
    let pool = pool.clone();
    let account_owned = account_id.to_string();
    let session_owned = session_id.map(|s| s.to_string());
    let query_owned = query.to_string();
    let task = tokio::task::spawn_blocking(move || {
        completion_rag_matches(
            &pool,
            &account_owned,
            session_owned.as_deref(),
            &query_owned,
        )
    });
    match tokio::time::timeout(budget, task).await {
        Ok(Ok(matches)) => matches,
        Ok(Err(join_err)) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                error = %join_err,
                "RAG retrieval task failed; continuing without retrieved context"
            );
            Vec::new()
        }
        Err(_elapsed) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                budget_ms = budget.as_millis() as u64,
                "RAG retrieval exceeded budget; continuing without retrieved context"
            );
            Vec::new()
        }
    }
}

fn first_token_deadline() -> std::time::Duration {
    let ms = std::env::var("BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .unwrap_or(DEFAULT_FIRST_TOKEN_TIMEOUT_MS);
    std::time::Duration::from_millis(ms)
}

fn missing_provider_key_error(provider: &str) -> anyhow::Error {
    anyhow::anyhow!("{provider} API key pool is not configured on bluey-server")
}

const MAX_COMPLETE_IMAGE_DATA_URLS: usize = 4;
const MAX_COMPLETE_IMAGE_DATA_URL_BYTES: usize = 12 * 1024 * 1024;
const ESTIMATED_TOKENS_PER_IMAGE: i64 = 1_500;

fn image_validation_error(
    error: impl Into<String>,
    reason: impl Into<String>,
) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: error.into(),
            reason: Some(reason.into()),
            ..Default::default()
        }),
    )
}

fn validate_complete_images(image_data_urls: &[String]) -> Result<(), ApiError> {
    if image_data_urls.len() > MAX_COMPLETE_IMAGE_DATA_URLS {
        return Err(ApiError {
            error: format!("too many screen images; maximum is {MAX_COMPLETE_IMAGE_DATA_URLS}"),
            reason: Some("too_many_images".into()),
            ..Default::default()
        });
    }

    for data_url in image_data_urls {
        if data_url.len() > MAX_COMPLETE_IMAGE_DATA_URL_BYTES {
            return Err(ApiError {
                error: "screen image is too large".into(),
                reason: Some("image_too_large".into()),
                ..Default::default()
            });
        }
        let allowed = data_url.starts_with("data:image/png;base64,")
            || data_url.starts_with("data:image/jpeg;base64,")
            || data_url.starts_with("data:image/webp;base64,")
            || data_url.starts_with("data:image/gif;base64,");
        if !allowed {
            return Err(ApiError {
                error: "unsupported screen image payload".into(),
                reason: Some("unsupported_image_payload".into()),
                ..Default::default()
            });
        }
    }

    Ok(())
}

fn image_token_estimate(image_count: usize) -> i64 {
    i64::try_from(image_count)
        .unwrap_or(i64::MAX / ESTIMATED_TOKENS_PER_IMAGE)
        .saturating_mul(ESTIMATED_TOKENS_PER_IMAGE)
}

fn priced_routes_for(
    lane: &str,
    estimated_input_tokens: i64,
    max_output_tokens: i64,
) -> Vec<PricedRoute> {
    routing::resolve_route_candidates(lane)
        .into_iter()
        .filter_map(|(provider, model)| {
            pricing::lookup(provider, model).map(|entry| {
                let mut route_pricing = *entry;
                if lane == "deep" {
                    route_pricing.markup_percent = 150;
                }
                PricedRoute {
                    provider,
                    model,
                    pricing: route_pricing,
                    estimated_cost_cents: pricing::estimate_cost_ceiling(
                        &route_pricing,
                        estimated_input_tokens,
                        max_output_tokens,
                    ),
                    estimated_bluey_cost_cents: pricing::estimate_bluey_cost_ceiling(
                        &route_pricing,
                        estimated_input_tokens,
                        max_output_tokens,
                    ),
                }
            })
        })
        .collect()
}

fn priced_transcribe_routes_for(
    deepgram_model: Option<&str>,
    estimated_seconds: i64,
) -> Vec<PricedTranscribeRoute> {
    routing::resolve_transcribe_candidates(deepgram_model)
        .into_iter()
        .filter_map(|(provider, model)| {
            pricing::lookup(provider, &model).map(|entry| PricedTranscribeRoute {
                provider,
                model,
                pricing: entry,
                estimated_cost_cents: pricing::estimate_cost_ceiling(entry, estimated_seconds, 0),
                estimated_bluey_cost_cents: pricing::estimate_bluey_cost_ceiling(
                    entry,
                    estimated_seconds,
                    0,
                ),
            })
        })
        .collect()
}

fn completion_rag_matches(
    pool: &crate::db::DbPool,
    account_id: &str,
    session_id: Option<&str>,
    query: &str,
) -> Vec<sync::RagMatch> {
    if query.trim().chars().count() < 8 {
        return Vec::new();
    }
    let mut matches = match sync::query_rag(pool, account_id, query, None, 12) {
        Ok(matches) => matches,
        Err(e) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                error = %e,
                "managed cloud RAG lookup failed; continuing without retrieved context"
            );
            return Vec::new();
        }
    };
    matches.sort_by(|a, b| {
        rag_completion_score(b, session_id).total_cmp(&rag_completion_score(a, session_id))
    });
    matches.truncate(6);
    matches
}

fn rag_completion_score(hit: &sync::RagMatch, session_id: Option<&str>) -> f32 {
    let current_session_boost = match (hit.session_id.as_deref(), session_id) {
        (Some(hit_session), Some(current_session)) if hit_session == current_session => 0.18,
        _ => 0.0,
    };
    hit.score + current_session_boost
}

fn prompt_with_rag_context(
    system: &str,
    user: &str,
    matches: &[sync::RagMatch],
) -> (String, String) {
    if matches.is_empty() {
        return (system.to_string(), user.to_string());
    }

    let mut context = String::from(
        "Relevant Bluey knowledge base snippets from user-approved sessions and attachments:\n",
    );
    for (idx, hit) in matches.iter().enumerate() {
        let label = rag_source_label(hit);
        let snippet = truncate_chars(hit.text.trim(), 900);
        context.push_str(&format!(
            "\n[S{}] {} · score {:.2}\n{}\n",
            idx + 1,
            label,
            hit.score,
            snippet
        ));
    }

    let system = format!(
        "{system}\n\n{context}\nUse these snippets only when relevant. Prefer the live user question when it conflicts with older memory. Do not expose snippet ids or source labels unless the user asks for sources."
    );
    (system, user.to_string())
}

fn rag_source_label(hit: &sync::RagMatch) -> String {
    let session = hit
        .session_id
        .as_deref()
        .map(|session_id| format!("session {session_id}"))
        .unwrap_or_else(|| "global memory".to_string());
    format!(
        "{} {} chunk {} ({session})",
        hit.source_kind, hit.source_id, hit.chunk_index
    )
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

pub async fn complete(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    Json(req): Json<CompleteRequest>,
) -> Result<Json<CompleteResponse>, (StatusCode, Json<ApiError>)> {
    Ok(Json(complete_inner(state, account, req, trace_id).await?))
}

/// Streaming variant of `/router/complete`.
///
/// This path performs the same entry checks/idempotency reservation as the
/// non-streaming endpoint, then proxies provider deltas as they arrive. Billing,
/// usage recording, and idempotency caching happen only after the upstream
/// stream finishes, followed by one `billing` SSE event carrying the final
/// `CompleteResponse`.
pub async fn complete_stream(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    Json(req): Json<CompleteRequest>,
) -> Result<Sse<RouterSseStream>, (StatusCode, Json<ApiError>)> {
    complete_stream_inner(state, account, req, trace_id).await
}

async fn complete_stream_inner(
    state: AppState,
    account: Account,
    req: CompleteRequest,
    trace_id: String,
) -> Result<Sse<RouterSseStream>, (StatusCode, Json<ApiError>)> {
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

    validate_complete_images(&req.image_data_urls)
        .map_err(|error| image_validation_error(error.error, error.reason.unwrap_or_default()))?;

    let effective_lane = if req.image_data_urls.is_empty() {
        req.lane.as_str()
    } else {
        "vision"
    };
    if effective_lane == "local" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "local lane is daemon-only; managed cloud does not run local models".into(),
                reason: Some("local_lane_unsupported".into()),
                ..Default::default()
            }),
        ));
    }

    let account_id_hash = cue_core::account_id_hash_prefix(&account.id);
    let session_id_log = log_session_id(req.session_id.as_deref()).to_string();
    let lane_log = req.lane.clone();
    let effective_lane_log = effective_lane.to_string();
    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        lane = %lane_log,
        effective_lane = %effective_lane_log,
        streaming = true,
        image_count = req.image_data_urls.len(),
        "managed chat request accepted"
    );

    match idempotency::reserve(&state.pool, &account.id, &req.request_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("idempotency: {e}"),
                ..Default::default()
            }),
        )
    })? {
        idempotency::ReserveOutcome::FreshReservation => {
            tracing::debug!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = true,
                "managed chat idempotency reserved"
            );
        }
        idempotency::ReserveOutcome::CachedComplete(json) => {
            let cached: CompleteResponse = serde_json::from_str(&json).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: format!("idempotency cache decode: {e}"),
                        ..Default::default()
                    }),
                )
            })?;
            tracing::info!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = true,
                "managed chat idempotency replayed completed response"
            );
            let events = response_to_sse_events(cached);
            return Ok(Sse::new(Box::pin(stream::iter(events.into_iter().map(Ok)))));
        }
        idempotency::ReserveOutcome::InProgress => {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = true,
                "managed chat duplicate request still in progress"
            );
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
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = true,
                "managed chat duplicate request previously failed terminally"
            );
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

    if let Err(denied) = state.rate_limiters.check_account_llm(&account.id).await {
        return Err(release_and_capacity_error(
            &state.pool,
            &account.id,
            &req.request_id,
            denied.reason,
            denied.retry_after_secs,
        ));
    }

    let rag_matches = completion_rag_matches_budgeted(
        &state.pool,
        &account.id,
        req.session_id.as_deref(),
        &req.user,
    )
    .await;
    tracing::debug!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        rag_match_count = rag_matches.len(),
        streaming = true,
        "managed chat memory context prepared"
    );
    let (provider_system, provider_user) =
        prompt_with_rag_context(&req.system, &req.user, &rag_matches);

    let thinking = routing::resolve_thinking_budget(
        effective_lane,
        req.reasoning_effort.as_deref(),
        req.thinking_budget_tokens,
    );
    let has_thinking_budget = !matches!(thinking.mode, routing::ThinkingMode::Off);
    let first_output_deadline = first_token_deadline_for_lane(effective_lane, has_thinking_budget);
    let effective_max_out = routing::effective_max_output_tokens(req.max_tokens, thinking);
    let max_out = i64::from(effective_max_out);
    let est_in = req
        .estimated_input_tokens
        .unwrap_or_else(|| ((provider_system.len() + provider_user.len()) as i64) / 4)
        + image_token_estimate(req.image_data_urls.len());
    let routes = priced_routes_for(effective_lane, est_in, max_out);
    if routes.is_empty() {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("no priced route for lane {effective_lane}"),
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
        &req.request_id,
        est_bluey_cost,
        "llm_stream",
    ) {
        return Err(err);
    }

    let on_trial = account.trial_seconds_remaining > 0;
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

    let started = Instant::now();
    let mut last_error: Option<anyhow::Error> = None;
    let mut last_capacity: Option<crate::rate_limit::CapacityDenied> = None;
    let mut last_failure_was_capacity = false;
    let mut selected_route_idx = 0usize;
    let mut selected_route: Option<PricedRoute> = None;
    let mut selected_stream: Option<routing::StreamingCompletion> = None;
    let mut selected_first_event: Option<anyhow::Result<routing::CompletionStreamEvent>> = None;

    for (idx, route) in routes.iter().enumerate() {
        let key_candidates = state.config.upstream.key_candidates(
            route.provider,
            &format!(
                "llm-stream:{}:{}:{}",
                req.request_id, route.provider, route.model
            ),
        );
        if key_candidates.is_empty() {
            last_error = Some(missing_provider_key_error(route.provider));
            continue;
        }

        loop {
            let selected_key = match state
                .provider_health
                .choose_key(route.provider, route.model, &key_candidates)
                .await
            {
                Ok(key) => key,
                Err(denied) => {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        retry_after_secs = denied.retry_after_secs,
                        reason = denied.reason,
                        "provider key pool cooling down; trying next streaming route"
                    );
                    last_capacity = Some(denied);
                    last_failure_was_capacity = true;
                    break;
                }
            };

            if let Err(denied) = state
                .rate_limiters
                .check_provider_llm(route.provider, route.model)
                .await
            {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    provider = %route.provider,
                    model = %route.model,
                    retry_after_secs = denied.retry_after_secs,
                    reason = denied.reason,
                    "provider capacity busy; trying next streaming route"
                );
                last_capacity = Some(denied);
                last_failure_was_capacity = true;
                break;
            }

            match routing::complete_stream_with_key(
                &selected_key.secret,
                route.provider,
                route.model,
                &provider_system,
                &provider_user,
                req.max_tokens,
                req.temperature,
                thinking,
                Some(est_in),
                &req.image_data_urls,
            )
            .await
            {
                Ok(streaming) => {
                    // B2: a 2xx connection is not yet a usable stream. Wait for
                    // the first event under a deadline. Any response (delta,
                    // terminal Done, in-band error, or empty stream) commits
                    // this route and is replayed through the consume loop
                    // unchanged. Only a stall (no event within the deadline)
                    // falls back to the next route instead of hanging.
                    let routing::StreamingCompletion {
                        provider: stream_provider,
                        model: stream_model,
                        events: mut stream_events,
                    } = streaming;
                    match tokio::time::timeout(first_output_deadline, stream_events.next()).await {
                        Ok(first_event) => {
                            selected_route_idx = idx;
                            selected_route = Some(*route);
                            tracing::info!(
                                account_id_hash = %account_id_hash,
                                request_id = %req.request_id,
                                session_id = %session_id_log,
                                lane = %lane_log,
                                effective_lane = %effective_lane_log,
                                provider = %route.provider,
                                model = %route.model,
                                route_index = idx,
                                was_fallback = idx > 0,
                                streaming = true,
                                "managed chat route selected"
                            );
                            selected_first_event = first_event;
                            selected_stream = Some(routing::StreamingCompletion {
                                provider: stream_provider,
                                model: stream_model,
                                events: stream_events,
                            });
                            break;
                        }
                        Err(_elapsed) => {
                            tracing::warn!(
                                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                                request_id = %req.request_id,
                                provider = %route.provider,
                                model = %route.model,
                                first_token_timeout_ms = first_output_deadline.as_millis() as u64,
                                "streaming first-token deadline exceeded; trying next route"
                            );
                            last_error = Some(anyhow::anyhow!("first-token deadline exceeded"));
                            last_failure_was_capacity = false;
                            break;
                        }
                    }
                }
                Err(e) => {
                    if let Some(retry_after_secs) = routing::upstream_retry_after(&e) {
                        let cooldown_secs = state
                            .provider_health
                            .record_cooldown(
                                route.provider,
                                route.model,
                                &selected_key.fingerprint,
                                retry_after_secs,
                            )
                            .await;
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            provider = %route.provider,
                            model = %route.model,
                            key_fingerprint = %selected_key.fingerprint,
                            retry_after_secs = cooldown_secs,
                            error = %e,
                            "streaming upstream capacity response; cooled key and retrying route"
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
                        request_id = %req.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        error = %e,
                        "streaming upstream dispatch failed; trying next route"
                    );
                    last_error = Some(e);
                    last_failure_was_capacity = false;
                    break;
                }
            }
        }

        if selected_stream.is_some() {
            break;
        }
    }

    let (selected_route, streaming) = match (selected_route, selected_stream) {
        (Some(route), Some(streaming)) => (route, streaming),
        _ => {
            let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
            if last_failure_was_capacity {
                let denied = last_capacity.expect("capacity flag set with no capacity denial");
                return Err(capacity_error(denied.reason, denied.retry_after_secs));
            }
            if let Some(e) = last_error {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %e,
                    "all streaming upstream dispatch routes failed"
                );
            }
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

    let idempotency_guard = StreamingIdempotencyGuard::new(
        state.pool.clone(),
        account.id.clone(),
        req.request_id.clone(),
    );
    let event_stream = async_stream::stream! {
        let mut idempotency_guard = idempotency_guard;
        let mut events = streaming.events;
        let mut pending_first = selected_first_event;
        let mut text = String::new();
        let mut final_tokens: Option<(i64, i64)> = None;
        let mut delivered_delta = false;

        loop {
            // B2: replay the event prefetched during the first-token deadline
            // check before resuming the live stream.
            let event = match pending_first.take() {
                Some(first) => Some(first),
                None => events.next().await,
            };
            let Some(event) = event else { break };
            match event {
                Ok(routing::CompletionStreamEvent::Delta(delta)) => {
                    text.push_str(&delta);
                    if !delivered_delta {
                        delivered_delta = true;
                        idempotency_guard.keep_in_progress_for_manual_reconciliation();
                    }
                    yield Ok(Event::default().data(
                        serde_json::json!({
                            "choices": [
                                { "delta": { "content": delta } }
                            ]
                        })
                        .to_string(),
                    ));
                }
                Ok(routing::CompletionStreamEvent::Done { input_tokens, output_tokens }) => {
                    final_tokens = Some((input_tokens, output_tokens));
                    break;
                }
                Err(e) => {
                    if delivered_delta {
                        idempotency_guard.mark_failed_now();
                    } else {
                        idempotency_guard.release_now();
                    }
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        error = %e,
                        delivered_delta,
                        "streaming upstream read failed"
                    );
                    yield Ok(Event::default().event("error").data(
                        serde_json::json!({
                            "error": "upstream provider stream interrupted; please retry",
                            "reason": "upstream_stream_error",
                        })
                        .to_string(),
                    ));
                    return;
                }
            }
        }

        let Some((input_tokens, output_tokens)) = final_tokens else {
            if delivered_delta {
                idempotency_guard.mark_failed_now();
            } else {
                idempotency_guard.release_now();
            }
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                delivered_delta,
                "streaming provider ended without a terminal billing event"
            );
            yield Ok(Event::default().event("error").data(
                serde_json::json!({
                    "error": "upstream provider stream ended before completion; please retry",
                    "reason": "upstream_stream_incomplete",
                })
                .to_string(),
            ));
            return;
        };
        let elapsed_ms = started.elapsed().as_millis() as i64;
        let (bluey_cost, customer_cost) = pricing::compute_cost(
            &selected_route.pricing,
            input_tokens,
            output_tokens,
        );

        let trial_remaining = if on_trial {
            match balance::consume_trial_seconds(&state.pool, &account.id, elapsed_ms) {
                Ok(value) => value,
                Err(e) => {
                    idempotency_guard.mark_failed_now();
                    yield Ok(Event::default().event("error").data(
                        serde_json::json!({
                            "error": format!("trial: {e}"),
                            "reason": "trial_update_failed",
                        })
                        .to_string(),
                    ));
                    return;
                }
            }
        } else {
            match balance::deduct(&state.pool, &account.id, customer_cost) {
                Ok(ok) => {
                    if ok {
                        account.trial_seconds_remaining
                    } else {
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            cost_cents = customer_cost,
                            "streaming post-completion deduct failed"
                        );
                        idempotency_guard.mark_failed_now();
                        yield Ok(Event::default().event("error").data(
                            serde_json::json!({
                                "error": "insufficient balance after completion; add credits and retry",
                                "reason": "insufficient_balance",
                                "reload_url": format!("{}/reload", state.config.public_url),
                            })
                            .to_string(),
                        ));
                        return;
                    }
                }
                Err(e) => {
                    idempotency_guard.mark_failed_now();
                    yield Ok(Event::default().event("error").data(
                        serde_json::json!({
                            "error": format!("deduct: {e}"),
                            "reason": "deduct_failed",
                        })
                        .to_string(),
                    ));
                    return;
                }
            }
        };

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
            request_id: req.request_id.clone(),
            kind: "llm".into(),
            task_type: None,
            lane: Some(req.lane.clone()),
            provider: Some(streaming.provider.clone()),
            model: Some(streaming.model.clone()),
            input_tokens,
            output_tokens,
            latency_ms: elapsed_ms,
            cost_cents_to_bluey: bluey_cost,
            cost_cents_to_customer: customer_cost,
            was_speculative: false,
            was_fallback: selected_route_idx > 0,
        };
        match usage::record(&state.pool, &account.id, &event) {
            Ok(true) => tracing::info!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                trace_id = %trace_id,
                session_id = %session_id_log,
                provider = %streaming.provider,
                model = %streaming.model,
                cost_cents = customer_cost,
                balance_cents_after = balance_after,
                latency_ms = elapsed_ms,
                streaming = true,
                "managed chat usage event recorded"
            ),
            Ok(false) => tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                trace_id = %trace_id,
                session_id = %session_id_log,
                provider = %streaming.provider,
                model = %streaming.model,
                streaming = true,
                "managed chat usage event deduplicated"
            ),
            Err(e) => tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                trace_id = %trace_id,
                session_id = %session_id_log,
                provider = %streaming.provider,
                model = %streaming.model,
                error = %e,
                streaming = true,
                "failed to record managed chat usage event"
            ),
        }

        tracing::info!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            session_id = %session_id_log,
            lane = %lane_log,
            effective_lane = %effective_lane_log,
            provider = %streaming.provider,
            model = %streaming.model,
            input_tokens,
            output_tokens,
            cost_cents = customer_cost,
            balance_cents_after = balance_after,
            trial_seconds_remaining = trial_remaining,
            latency_ms = elapsed_ms,
            was_fallback = selected_route_idx > 0,
            streaming = true,
            "managed chat completed and billed"
        );

        let artifact = response_artifact(&text);
        let response = CompleteResponse {
            text,
            provider: streaming.provider,
            model: streaming.model,
            input_tokens,
            output_tokens,
            cost_cents: customer_cost,
            balance_cents_after: balance_after,
            trial_seconds_remaining: trial_remaining,
            artifact_type: artifact
                .as_ref()
                .map(|artifact| artifact.artifact_type.to_string()),
            artifact_body: artifact.as_ref().map(|artifact| artifact.body.clone()),
            cost_label: Some(router_cost_label(customer_cost, balance_after)),
            confidence: artifact.as_ref().map(|artifact| artifact.confidence),
        };

        match serde_json::to_string(&response) {
            Ok(json) => {
                if let Err(e) = idempotency_guard.mark_complete_now(&json) {
                    tracing::error!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        error = %e,
                        "streaming idempotency::mark_complete failed AFTER customer billed; retry will return 409 — manual reconciliation required"
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %e,
                    "failed to serialize streaming response for idempotency cache; retry will return 409 — manual reconciliation required"
                );
                idempotency_guard.keep_in_progress_for_manual_reconciliation();
            }
        }

        let billing = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());
        yield Ok(Event::default().event("billing").data(billing));
        yield Ok(Event::default().data("[DONE]"));
    };

    Ok(Sse::new(Box::pin(event_stream)))
}

async fn complete_inner(
    state: AppState,
    account: Account,
    req: CompleteRequest,
    trace_id: String,
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

    validate_complete_images(&req.image_data_urls)
        .map_err(|error| image_validation_error(error.error, error.reason.unwrap_or_default()))?;

    let effective_lane = if req.image_data_urls.is_empty() {
        req.lane.as_str()
    } else {
        "vision"
    };

    // Codex S4.4: managed dispatcher does not run local models.
    // The daemon's LocalFallbackPolicy must dispatch local-lane work
    // directly to on-device Ollama; the managed cloud path is not the
    // right home for it.
    if effective_lane == "local" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "local lane is daemon-only; managed cloud does not run local models".into(),
                reason: Some("local_lane_unsupported".into()),
                ..Default::default()
            }),
        ));
    }

    let account_id_hash = cue_core::account_id_hash_prefix(&account.id);
    let session_id_log = log_session_id(req.session_id.as_deref()).to_string();
    let lane_log = req.lane.clone();
    let effective_lane_log = effective_lane.to_string();
    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        lane = %lane_log,
        effective_lane = %effective_lane_log,
        streaming = false,
        image_count = req.image_data_urls.len(),
        "managed chat request accepted"
    );

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
        idempotency::ReserveOutcome::FreshReservation => {
            tracing::debug!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat idempotency reserved"
            );
        }
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
            tracing::info!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat idempotency replayed completed response"
            );
            return Ok(cached);
        }
        idempotency::ReserveOutcome::InProgress => {
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat duplicate request still in progress"
            );
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
            tracing::warn!(
                account_id_hash = %account_id_hash,
                request_id = %req.request_id,
                session_id = %session_id_log,
                streaming = false,
                "managed chat duplicate request previously failed terminally"
            );
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

    if let Err(denied) = state.rate_limiters.check_account_llm(&account.id).await {
        return Err(release_and_capacity_error(
            &state.pool,
            &account.id,
            &req.request_id,
            denied.reason,
            denied.retry_after_secs,
        ));
    }

    let rag_matches = completion_rag_matches_budgeted(
        &state.pool,
        &account.id,
        req.session_id.as_deref(),
        &req.user,
    )
    .await;
    tracing::debug!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        rag_match_count = rag_matches.len(),
        streaming = false,
        "managed chat memory context prepared"
    );
    let (provider_system, provider_user) =
        prompt_with_rag_context(&req.system, &req.user, &rag_matches);

    // 2. Resolve lane → provider+model candidates. Entry balance check uses
    // the maximum candidate estimate so provider failover cannot overrun a
    // customer's hard-stop budget.
    let thinking = routing::resolve_thinking_budget(
        effective_lane,
        req.reasoning_effort.as_deref(),
        req.thinking_budget_tokens,
    );
    let effective_max_out = routing::effective_max_output_tokens(req.max_tokens, thinking);
    let max_out = i64::from(effective_max_out);
    let est_in = req.estimated_input_tokens.unwrap_or_else(|| {
        // Crude fallback: ~4 chars/token
        ((provider_system.len() + provider_user.len()) as i64) / 4
    }) + image_token_estimate(req.image_data_urls.len());
    let routes = priced_routes_for(effective_lane, est_in, max_out);
    if routes.is_empty() {
        let _ = idempotency::mark_failed(&state.pool, &account.id, &req.request_id);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("no priced route for lane {effective_lane}"),
                ..Default::default()
            }),
        ));
    }

    // 3. Estimate cost ceiling for the entry check.
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
        &req.request_id,
        est_bluey_cost,
        "llm",
    ) {
        return Err(err);
    }

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
                    ..Default::default()
                }),
            ));
        }
    }

    // 5. Dispatch to upstream provider. Try candidate routes in order. Provider
    //    capacity is checked before each attempt, so a provider 429/rate-limit
    //    storm degrades to another route instead of failing the active call.
    //    Pass the entry estimate so the dispatcher can fall back to it if the
    //    upstream omits `usage`. Codex S4.5.
    let started = Instant::now();
    let mut last_error: Option<anyhow::Error> = None;
    let mut last_capacity: Option<crate::rate_limit::CapacityDenied> = None;
    let mut last_failure_was_capacity = false;
    let mut selected_route_idx = 0usize;
    let mut selected_route: Option<&PricedRoute> = None;
    let mut selected_completion: Option<routing::Completion> = None;

    for (idx, route) in routes.iter().enumerate() {
        let key_candidates = state.config.upstream.key_candidates(
            route.provider,
            &format!("llm:{}:{}:{}", req.request_id, route.provider, route.model),
        );
        if key_candidates.is_empty() {
            last_error = Some(missing_provider_key_error(route.provider));
            continue;
        }

        loop {
            let selected_key = match state
                .provider_health
                .choose_key(route.provider, route.model, &key_candidates)
                .await
            {
                Ok(key) => key,
                Err(denied) => {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        request_id = %req.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        retry_after_secs = denied.retry_after_secs,
                        reason = denied.reason,
                        "provider key pool cooling down; trying next route"
                    );
                    last_capacity = Some(denied);
                    last_failure_was_capacity = true;
                    break;
                }
            };

            if let Err(denied) = state
                .rate_limiters
                .check_provider_llm(route.provider, route.model)
                .await
            {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    provider = %route.provider,
                    model = %route.model,
                    retry_after_secs = denied.retry_after_secs,
                    reason = denied.reason,
                    "provider capacity busy; trying next route"
                );
                last_capacity = Some(denied);
                last_failure_was_capacity = true;
                break;
            }

            match routing::complete_with_key(
                &selected_key.secret,
                route.provider,
                route.model,
                &provider_system,
                &provider_user,
                req.max_tokens,
                req.temperature,
                thinking,
                Some(est_in),
                &req.image_data_urls,
            )
            .await
            {
                Ok(completion) => {
                    selected_route_idx = idx;
                    selected_route = Some(route);
                    tracing::info!(
                        account_id_hash = %account_id_hash,
                        request_id = %req.request_id,
                        session_id = %session_id_log,
                        lane = %lane_log,
                        effective_lane = %effective_lane_log,
                        provider = %route.provider,
                        model = %route.model,
                        route_index = idx,
                        was_fallback = idx > 0,
                        streaming = false,
                        "managed chat route selected"
                    );
                    selected_completion = Some(completion);
                    break;
                }
                Err(e) => {
                    if let Some(retry_after_secs) = routing::upstream_retry_after(&e) {
                        let cooldown_secs = state
                            .provider_health
                            .record_cooldown(
                                route.provider,
                                route.model,
                                &selected_key.fingerprint,
                                retry_after_secs,
                            )
                            .await;
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            request_id = %req.request_id,
                            provider = %route.provider,
                            model = %route.model,
                            key_fingerprint = %selected_key.fingerprint,
                            retry_after_secs = cooldown_secs,
                            error = %e,
                            "upstream capacity response; cooled key and retrying route"
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
                        request_id = %req.request_id,
                        provider = %route.provider,
                        model = %route.model,
                        error = %e,
                        "upstream dispatch failed; trying next route"
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
    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_millis() as i64;

    let (selected_route, comp) = match (selected_route, selected_completion) {
        (Some(route), Some(completion)) => (route, completion),
        _ => {
            let _ = idempotency::release(&state.pool, &account.id, &req.request_id);
            if last_failure_was_capacity {
                let denied = last_capacity.expect("capacity flag set with no capacity denial");
                return Err(capacity_error(denied.reason, denied.retry_after_secs));
            }
            if let Some(e) = last_error {
                // Codex S4.6: log raw upstream details, return sanitized
                // message to the customer. We DO NOT mark the idempotency
                // row as failed-terminal because a transient upstream error
                // should be retryable with the same request_id.
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %e,
                    "all upstream dispatch routes failed"
                );
            }
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
    let (bluey_cost, customer_cost) = pricing::compute_cost(
        &selected_route.pricing,
        comp.input_tokens,
        comp.output_tokens,
    );

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
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
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
        was_fallback: selected_route_idx > 0,
    };
    match crate::db::usage::record(&state.pool, &account.id, &event) {
        Ok(true) => tracing::info!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            trace_id = %trace_id,
            session_id = %session_id_log,
            provider = %comp.provider,
            model = %comp.model,
            cost_cents = customer_cost,
            balance_cents_after = balance_after,
            latency_ms = elapsed_ms,
            streaming = false,
            "managed chat usage event recorded"
        ),
        Ok(false) => tracing::warn!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            trace_id = %trace_id,
            session_id = %session_id_log,
            provider = %comp.provider,
            model = %comp.model,
            streaming = false,
            "managed chat usage event deduplicated"
        ),
        Err(e) => tracing::warn!(
            account_id_hash = %account_id_hash,
            request_id = %req.request_id,
            trace_id = %trace_id,
            session_id = %session_id_log,
            provider = %comp.provider,
            model = %comp.model,
            error = %e,
            streaming = false,
            "failed to record managed chat usage event"
        ),
    }

    tracing::info!(
        account_id_hash = %account_id_hash,
        request_id = %req.request_id,
        session_id = %session_id_log,
        lane = %lane_log,
        effective_lane = %effective_lane_log,
        provider = %comp.provider,
        model = %comp.model,
        input_tokens = comp.input_tokens,
        output_tokens = comp.output_tokens,
        cost_cents = customer_cost,
        balance_cents_after = balance_after,
        trial_seconds_remaining = trial_remaining,
        latency_ms = elapsed_ms,
        was_fallback = selected_route_idx > 0,
        streaming = false,
        "managed chat completed and billed"
    );

    let artifact = response_artifact(&comp.text);
    let response = CompleteResponse {
        text: comp.text,
        provider: comp.provider,
        model: comp.model,
        input_tokens: comp.input_tokens,
        output_tokens: comp.output_tokens,
        cost_cents: customer_cost,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_remaining,
        artifact_type: artifact
            .as_ref()
            .map(|artifact| artifact.artifact_type.to_string()),
        artifact_body: artifact.as_ref().map(|artifact| artifact.body.clone()),
        cost_label: Some(router_cost_label(customer_cost, balance_after)),
        confidence: artifact.as_ref().map(|artifact| artifact.confidence),
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
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    request_id = %req.request_id,
                    error = %e,
                    "idempotency::mark_complete failed AFTER customer billed; retry will return 409 — manual reconciliation required"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                request_id = %req.request_id,
                error = %e,
                "failed to serialize response for idempotency cache; retry will return 409 — manual reconciliation required"
            );
        }
    }

    Ok(response)
}

struct ResponseArtifact {
    artifact_type: &'static str,
    body: String,
    confidence: f32,
}

fn response_artifact(text: &str) -> Option<ResponseArtifact> {
    let body = text.trim();
    if body.is_empty() {
        return None;
    }

    let lower = body.to_lowercase();
    let code_blocks = extract_fenced_code_blocks(body);
    if !code_blocks.is_empty() || has_code_shape(&lower) {
        return Some(ResponseArtifact {
            artifact_type: "code",
            body: format_code_artifact(body, &code_blocks),
            confidence: if code_blocks.is_empty() { 0.74 } else { 0.95 },
        });
    }
    if keyword_count(
        &lower,
        &[
            "system design",
            "architecture",
            "api",
            "database",
            "cache",
            "queue",
            "scale",
            "latency",
            "throughput",
            "tradeoff",
            "load balancer",
            "microservice",
        ],
    ) >= 3
    {
        return Some(ResponseArtifact {
            artifact_type: "system_design",
            body: format_structured_artifact(body, "System Design"),
            confidence: 0.88,
        });
    }
    if lower.contains("screenshot")
        || lower.contains("screen context")
        || lower.contains("analyse screen")
        || lower.contains("analyze screen")
        || lower.contains("image shows")
    {
        return Some(ResponseArtifact {
            artifact_type: "screen",
            body: format_structured_artifact(body, "Screen Context"),
            confidence: 0.86,
        });
    }
    if lower.contains("attached document")
        || lower.contains("pdf")
        || lower.contains("resume")
        || lower.contains("document context")
    {
        return Some(ResponseArtifact {
            artifact_type: "document",
            body: format_structured_artifact(body, "Document Context"),
            confidence: 0.78,
        });
    }
    if body.chars().count() > 950 && has_structured_shape(body) {
        return Some(ResponseArtifact {
            artifact_type: "structured",
            body: format_structured_artifact(body, "Details"),
            confidence: 0.70,
        });
    }

    None
}

fn router_cost_label(cost_cents: i64, balance_cents_after: i64) -> String {
    format!(
        "${:.2} · balance ${:.2}",
        cost_cents as f64 / 100.0,
        balance_cents_after as f64 / 100.0
    )
}

fn keyword_count(text: &str, keywords: &[&str]) -> usize {
    keywords
        .iter()
        .filter(|keyword| text.contains(**keyword))
        .count()
}

fn has_code_shape(lower: &str) -> bool {
    keyword_count(
        lower,
        &[
            "class solution",
            "def ",
            "function ",
            "const ",
            "let ",
            "public ",
            "private ",
            "time complexity",
            "space complexity",
            "test case",
            "edge case",
            "sql",
        ],
    ) >= 2
}

fn extract_fenced_code_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_fence {
                let block = current.join("\n").trim().to_string();
                if !block.is_empty() {
                    blocks.push(block);
                }
                current.clear();
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            current.push(line);
        }
    }
    blocks
}

fn strip_fenced_code(text: &str) -> String {
    let mut lines = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn format_code_artifact(body: &str, code_blocks: &[String]) -> String {
    let notes = strip_fenced_code(body).trim().to_string();
    let mut sections = Vec::new();
    if !code_blocks.is_empty() {
        sections.push(format!(
            "CODE\n----\n{}",
            code_blocks.join("\n\n// ---\n\n")
        ));
    }
    if !notes.is_empty() {
        sections.push(format!("NOTES\n-----\n{notes}"));
    }
    if sections.is_empty() {
        body.to_string()
    } else {
        sections.join("\n\n")
    }
}

fn format_structured_artifact(body: &str, fallback_heading: &str) -> String {
    let clean = body.trim();
    if clean.starts_with('#')
        || clean
            .to_lowercase()
            .starts_with(&fallback_heading.to_lowercase())
    {
        clean.to_string()
    } else {
        format!(
            "{fallback_heading}\n{}\n{clean}",
            "-".repeat(fallback_heading.len())
        )
    }
}

fn has_structured_shape(text: &str) -> bool {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with('#')
                || numbered_list_prefix(trimmed)
        })
        .count()
        >= 3
}

fn numbered_list_prefix(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut saw_digit = false;
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        saw_digit = true;
        chars.next();
    }
    saw_digit
        && matches!(chars.next(), Some('.' | ')'))
        && matches!(chars.next(), Some(ch) if ch.is_whitespace())
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
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
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
    let est_in = (req.input.len() as i64) / 4;
    let est_cost = pricing::estimate_cost_ceiling(pricing_entry, est_in, 0);
    let est_bluey_cost = pricing::estimate_bluey_cost_ceiling(pricing_entry, est_in, 0);
    if let Some(err) = release_and_upstream_spend_guard_check(
        &state,
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

        match routing::embed_with_key(&selected_key.secret, provider, model, &req.input).await {
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
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
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
        tracing::warn!(trace_id = %trace_id, error = %e, "failed to record embed usage event");
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
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
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
        cost_cents: customer_cost,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> crate::db::DbPool {
        let path = std::env::temp_dir().join(format!("bluey-router-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &crate::db::DbPool, email: &str) -> String {
        crate::db::accounts::Account::create(pool, email, "stub")
            .unwrap()
            .id
    }

    #[test]
    fn rag_retrieval_budget_default_and_override() {
        std::env::remove_var("BLUEY_RAG_RETRIEVAL_BUDGET_MS");
        assert_eq!(
            rag_retrieval_budget(),
            std::time::Duration::from_millis(DEFAULT_RAG_RETRIEVAL_BUDGET_MS)
        );
        std::env::set_var("BLUEY_RAG_RETRIEVAL_BUDGET_MS", "50");
        assert_eq!(rag_retrieval_budget(), std::time::Duration::from_millis(50));
        std::env::set_var("BLUEY_RAG_RETRIEVAL_BUDGET_MS", "0");
        assert_eq!(
            rag_retrieval_budget(),
            std::time::Duration::from_millis(DEFAULT_RAG_RETRIEVAL_BUDGET_MS)
        );
        std::env::remove_var("BLUEY_RAG_RETRIEVAL_BUDGET_MS");
    }

    #[test]
    fn first_token_deadline_default_and_override() {
        std::env::remove_var("BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS");
        assert_eq!(
            first_token_deadline(),
            std::time::Duration::from_millis(DEFAULT_FIRST_TOKEN_TIMEOUT_MS)
        );
        std::env::set_var("BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS", "250");
        assert_eq!(
            first_token_deadline(),
            std::time::Duration::from_millis(250)
        );
        // Zero / invalid falls back to the default.
        std::env::set_var("BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS", "0");
        assert_eq!(
            first_token_deadline(),
            std::time::Duration::from_millis(DEFAULT_FIRST_TOKEN_TIMEOUT_MS)
        );
        std::env::set_var("BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS", "notnum");
        assert_eq!(
            first_token_deadline(),
            std::time::Duration::from_millis(DEFAULT_FIRST_TOKEN_TIMEOUT_MS)
        );
        std::env::remove_var("BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS");
    }

    #[test]
    fn deep_first_token_deadline_uses_deep_budget() {
        std::env::remove_var("BLUEY_STREAM_DEEP_FIRST_TOKEN_TIMEOUT_MS");
        assert_eq!(
            first_token_deadline_for_lane("deep", true),
            std::time::Duration::from_millis(DEFAULT_DEEP_FIRST_TOKEN_TIMEOUT_MS)
        );
        std::env::set_var("BLUEY_STREAM_DEEP_FIRST_TOKEN_TIMEOUT_MS", "12000");
        assert_eq!(
            first_token_deadline_for_lane("balanced", true),
            std::time::Duration::from_millis(12_000)
        );
        std::env::remove_var("BLUEY_STREAM_DEEP_FIRST_TOKEN_TIMEOUT_MS");
    }

    #[test]
    fn streaming_idempotency_guard_releases_on_drop_before_billing() {
        let pool = temp_pool();
        let account_id = make_account(&pool, "stream-drop@example.com");
        idempotency::reserve(&pool, &account_id, "stream-drop").unwrap();

        {
            let _guard = StreamingIdempotencyGuard::new(
                pool.clone(),
                account_id.clone(),
                "stream-drop".into(),
            );
        }

        assert_eq!(
            idempotency::reserve(&pool, &account_id, "stream-drop").unwrap(),
            idempotency::ReserveOutcome::FreshReservation
        );
    }

    #[test]
    fn streaming_idempotency_guard_can_preserve_in_progress_after_billing() {
        let pool = temp_pool();
        let account_id = make_account(&pool, "stream-reconcile@example.com");
        idempotency::reserve(&pool, &account_id, "stream-reconcile").unwrap();

        {
            let mut guard = StreamingIdempotencyGuard::new(
                pool.clone(),
                account_id.clone(),
                "stream-reconcile".into(),
            );
            guard.keep_in_progress_for_manual_reconciliation();
        }

        assert_eq!(
            idempotency::reserve(&pool, &account_id, "stream-reconcile").unwrap(),
            idempotency::ReserveOutcome::InProgress
        );
    }

    #[test]
    fn response_artifact_detects_code() {
        let artifact = response_artifact(
            "Use this implementation.\n```python\ndef solve():\n    return 42\n```\nTime Complexity: O(1)",
        )
        .expect("code artifact");

        assert_eq!(artifact.artifact_type, "code");
        assert!(artifact.body.contains("CODE\n----"));
        assert!(artifact.body.contains("def solve()"));
    }

    #[test]
    fn response_artifact_detects_system_design() {
        let artifact = response_artifact(
            "For this system design, use an API gateway, database, cache, queue, and load balancer to reduce latency at scale.",
        )
        .expect("system design artifact");

        assert_eq!(artifact.artifact_type, "system_design");
        assert!(artifact.confidence > 0.8);
    }

    #[test]
    fn router_cost_label_includes_balance() {
        assert_eq!(router_cost_label(7, 2993), "$0.07 · balance $29.93");
    }

    #[test]
    fn local_lane_has_no_managed_priced_routes() {
        assert!(
            priced_routes_for("local", 100, 100).is_empty(),
            "local/Ollama fallback must stay daemon-only, not managed cloud"
        );
    }

    #[test]
    fn deep_lane_fallbacks_keep_deep_markup() {
        let routes = priced_routes_for("deep", 1_000, 1_000);
        let sonnet_fallback = routes
            .iter()
            .find(|route| route.provider == "anthropic" && route.model.contains("sonnet"))
            .expect("deep lane keeps a Sonnet fallback");

        assert_eq!(sonnet_fallback.pricing.markup_percent, 150);
    }

    #[test]
    fn complete_image_validation_accepts_supported_data_urls() {
        let images = vec![
            "data:image/png;base64,aGVsbG8=".to_string(),
            "data:image/jpeg;base64,aGVsbG8=".to_string(),
            "data:image/webp;base64,aGVsbG8=".to_string(),
        ];
        assert!(validate_complete_images(&images).is_ok());
        assert_eq!(image_token_estimate(images.len()), 4_500);
    }

    #[test]
    fn complete_image_validation_rejects_unsupported_payload() {
        let images = vec!["file:///tmp/screenshot.png".to_string()];
        let error = validate_complete_images(&images).unwrap_err();
        assert_eq!(error.reason.as_deref(), Some("unsupported_image_payload"));
    }

    #[test]
    fn complete_image_validation_rejects_too_many_images() {
        let images = vec!["data:image/png;base64,aGVsbG8=".to_string(); 5];
        let error = validate_complete_images(&images).unwrap_err();
        assert_eq!(error.reason.as_deref(), Some("too_many_images"));
    }

    #[test]
    fn rag_completion_score_boosts_current_session() {
        let current = sync::RagMatch {
            chunk_id: "current".into(),
            session_id: Some("session-a".into()),
            source_kind: "transcript".into(),
            source_id: "seg-1".into(),
            chunk_index: 0,
            text: "current session cache plan".into(),
            score: 0.40,
            embedding_model: None,
        };
        let older = sync::RagMatch {
            chunk_id: "older".into(),
            session_id: Some("session-b".into()),
            source_kind: "context".into(),
            source_id: "doc-1".into(),
            chunk_index: 0,
            text: "older cache plan".into(),
            score: 0.50,
            embedding_model: None,
        };

        assert!(
            rag_completion_score(&current, Some("session-a"))
                > rag_completion_score(&older, Some("session-a"))
        );
    }

    #[test]
    fn prompt_with_rag_context_adds_memory_without_changing_user_text() {
        let matches = vec![sync::RagMatch {
            chunk_id: "chunk-1".into(),
            session_id: Some("session-a".into()),
            source_kind: "attached_doc".into(),
            source_id: "architecture.pdf".into(),
            chunk_index: 2,
            text: "Use write-through caching for the billing cache.".into(),
            score: 0.73,
            embedding_model: None,
        }];

        let (system, user) = prompt_with_rag_context(
            "You are Bluey.",
            "How should I describe the cache design?",
            &matches,
        );

        assert_eq!(user, "How should I describe the cache design?");
        assert!(system.contains("Relevant Bluey knowledge base snippets"));
        assert!(system.contains("write-through caching"));
        assert!(system.contains("Use these snippets only when relevant"));
    }

    #[test]
    fn transcribe_priced_routes_include_cloud_fallback() {
        let routes = priced_transcribe_routes_for(None, 60);
        let names: Vec<_> = routes
            .iter()
            .map(|route| (route.provider, route.model.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![("deepgram", "nova-3"), ("openai", "gpt-4o-mini-transcribe")]
        );
        assert!(routes.iter().all(|route| route.estimated_cost_cents > 0));
    }
}
