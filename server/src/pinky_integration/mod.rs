//! Additive, default-off Pinky integration. Standalone and Jobs auth are unchanged.

pub mod delegation;
mod store;

#[cfg(test)]
mod http_tests;

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, OriginalUri, State},
    http::{HeaderMap, StatusCode},
    middleware::from_fn,
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::post,
    Extension, Json, Router,
};
use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

use crate::api::router::{CompleteRequest, ManagedSettlementSignal, ManagedStreamPolicy};
use crate::db::DbPool;
use delegation::DelegationConfig;

const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_PROMPT_CHARS: usize = 4_000;
const MAX_SYNTHETIC_TEST_CREDIT_CENTS: i64 = 1_500;
const MAX_SYNTHETIC_ENTITLEMENT_TTL_SECONDS: i64 = 24 * 60 * 60;
const MAX_SYNTHETIC_TEST_SUBJECTS: usize = 16;

#[derive(Clone)]
pub struct IntegrationConfig {
    delegation: DelegationConfig,
    synthetic_entitlement: Option<store::SyntheticEntitlement>,
    synthetic_subject_keys: Arc<HashSet<String>>,
}

impl From<DelegationConfig> for IntegrationConfig {
    fn from(delegation: DelegationConfig) -> Self {
        Self {
            delegation,
            synthetic_entitlement: None,
            synthetic_subject_keys: Arc::new(HashSet::new()),
        }
    }
}

/// Explicit opt-in; absent/zero preserves the existing standalone router.
/// Integration credentials must be independent of standalone JWT signing.
pub fn config_from_env(standalone_secret: &str) -> anyhow::Result<Option<IntegrationConfig>> {
    config_from_lookup(standalone_secret, |name| std::env::var(name))
}

fn config_from_lookup(
    standalone_secret: &str,
    lookup: impl Fn(&str) -> Result<String, std::env::VarError>,
) -> anyhow::Result<Option<IntegrationConfig>> {
    match lookup("BLUEY_PINKY_INTEGRATION_ENABLED").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => return Ok(None),
        Ok("1") => {}
        _ => anyhow::bail!("invalid Pinky integration flag"),
    }
    let read = |name| {
        lookup(name).map_err(|_| anyhow::anyhow!("incomplete Pinky integration configuration"))
    };
    let secret = read("BLUEY_PINKY_DELEGATION_SECRET")?;
    if secret == standalone_secret {
        anyhow::bail!("Pinky delegation requires independent signing material");
    }
    let delegation = DelegationConfig::new(
        secret,
        read("BLUEY_PINKY_ISSUER")?,
        read("BLUEY_PINKY_AUDIENCE")?,
        read("BLUEY_PINKY_ENVIRONMENT")?,
    )?;
    let (synthetic_entitlement, synthetic_subject_keys) =
        match lookup("BLUEY_PINKY_PREPROD_TEST_ENTITLEMENT_ENABLED").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("0") => (None, HashSet::new()),
            Ok("1") if delegation.environment == "preprod" => {
                let credit_cents = read("BLUEY_PINKY_PREPROD_TEST_CREDIT_CENTS")?
                    .parse::<i64>()
                    .ok()
                    .filter(|value| (1..=MAX_SYNTHETIC_TEST_CREDIT_CENTS).contains(value))
                    .ok_or_else(|| anyhow::anyhow!("invalid Pinky synthetic test credit"))?;
                let ttl_seconds = read("BLUEY_PINKY_PREPROD_TEST_ENTITLEMENT_TTL_SECS")?
                    .parse::<i64>()
                    .ok()
                    .filter(|value| (60..=MAX_SYNTHETIC_ENTITLEMENT_TTL_SECONDS).contains(value))
                    .ok_or_else(|| anyhow::anyhow!("invalid Pinky synthetic entitlement expiry"))?;
                let raw_subjects = read("BLUEY_PINKY_PREPROD_TEST_SUBJECTS")?;
                let subjects = raw_subjects
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>();
                if subjects.is_empty() || subjects.len() > MAX_SYNTHETIC_TEST_SUBJECTS {
                    anyhow::bail!("invalid Pinky synthetic test subject allowlist");
                }
                let mut subject_keys = HashSet::with_capacity(subjects.len());
                for subject in subjects {
                    let id = uuid::Uuid::parse_str(subject)
                        .map_err(|_| anyhow::anyhow!("invalid Pinky synthetic test subject"))?;
                    if id.is_nil() || id.hyphenated().to_string() != subject {
                        anyhow::bail!("invalid Pinky synthetic test subject");
                    }
                    let key = store::subject_key(
                        &delegation.issuer,
                        &delegation.audience,
                        &delegation.environment,
                        subject,
                    );
                    if !subject_keys.insert(key) {
                        anyhow::bail!("duplicate Pinky synthetic test subject");
                    }
                }
                (
                    Some(store::SyntheticEntitlement {
                        credit_cents,
                        ttl_seconds,
                    }),
                    subject_keys,
                )
            }
            _ => anyhow::bail!("invalid Pinky preprod synthetic entitlement configuration"),
        };
    Ok(Some(IntegrationConfig {
        delegation,
        synthetic_entitlement,
        synthetic_subject_keys: Arc::new(synthetic_subject_keys),
    }))
}

#[derive(Clone)]
struct IntegrationState {
    pool: DbPool,
    delegation: Arc<DelegationConfig>,
    synthetic_entitlement: Option<store::SyntheticEntitlement>,
    synthetic_subject_keys: Arc<HashSet<String>>,
    managed: crate::api::AppState,
    start_limit: crate::rate_limit::Limiter,
    stop_limit: crate::rate_limit::Limiter,
    ask_limit: crate::rate_limit::Limiter,
    status_limit: crate::rate_limit::Limiter,
}

/// Called by main and the merged-router regression tests. None neither creates
/// integration tables nor installs delegated routes.
pub fn build_application(
    pool: DbPool,
    config: crate::config::Config,
    integration: Option<IntegrationConfig>,
) -> anyhow::Result<Router> {
    let managed = crate::api::AppState {
        pool: pool.clone(),
        config: Arc::new(config.clone()),
        rate_limiters: crate::rate_limit::RateLimiters::default(),
        provider_health: crate::provider_health::ProviderHealth::default(),
    };
    let app = crate::api::build_router(pool.clone(), config);
    match integration {
        Some(integration) => Ok(app.merge(routes_with_config(pool, managed, integration)?)),
        None => Ok(app),
    }
}

pub fn routes(
    pool: DbPool,
    delegation: DelegationConfig,
    config: crate::config::Config,
) -> anyhow::Result<Router> {
    let managed = crate::api::AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        rate_limiters: crate::rate_limit::RateLimiters::default(),
        provider_health: crate::provider_health::ProviderHealth::default(),
    };
    routes_with_config(pool, managed, delegation.into())
}

fn routes_with_config(
    pool: DbPool,
    managed: crate::api::AppState,
    integration: IntegrationConfig,
) -> anyhow::Result<Router> {
    store::initialize(&pool)?;
    let state = IntegrationState {
        pool,
        delegation: Arc::new(integration.delegation),
        synthetic_entitlement: integration.synthetic_entitlement,
        synthetic_subject_keys: integration.synthetic_subject_keys,
        managed,
        // One fixed key per route class: memory stays bounded even for bad
        // credentials. Separate Stop budget avoids starvation by Start traffic.
        // These are single-process preprod limits, not a distributed quota.
        start_limit: crate::rate_limit::Limiter::new(120, 30),
        stop_limit: crate::rate_limit::Limiter::new(120, 30),
        ask_limit: crate::rate_limit::Limiter::new(60, 10),
        status_limit: crate::rate_limit::Limiter::new(240, 60),
    };
    Ok(Router::new()
        .route("/integrations/pinky/sessions", post(open_session))
        .route("/integrations/pinky/sessions/close", post(close_session))
        .route("/integrations/pinky/ask/stream", post(ask_stream))
        .route("/integrations/pinky/asks/cancel", post(cancel_ask))
        .route("/integrations/pinky/asks/status", post(ask_status))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(from_fn(
            crate::api::middleware::request_id::request_id_middleware,
        ))
        .with_state(state))
}

enum AdmissionClass {
    Start,
    Stop,
    Ask,
    Status,
}

async fn admission(state: &IntegrationState, class: AdmissionClass) -> Result<(), ApiError> {
    let limit = match class {
        AdmissionClass::Start => &state.start_limit,
        AdmissionClass::Stop => &state.stop_limit,
        AdmissionClass::Ask => &state.ask_limit,
        AdmissionClass::Status => &state.status_limit,
    };
    if let Err(seconds) = limit.check("integration-edge").await {
        let mut response = error(StatusCode::TOO_MANY_REQUESTS, "integration_rate_limited");
        response.retry_after = Some(seconds);
        return Err(response);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRequest {
    session_id: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ResponseMode {
    Default,
    Short,
    Star,
}

impl ResponseMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Short => "short",
            Self::Star => "star",
        }
    }

    fn answer_rules(self) -> &'static str {
        match self {
            Self::Default => {
                "Answer the text question directly and concisely. Do not assume access to remote-session, screen, audio, files, prior conversation, or saved history."
            }
            Self::Short => {
                "Give a short direct answer in no more than 120 words: one compact paragraph or at most three short bullets. Compress any longer planner format into this limit. Use plain text, without Markdown headings, emphasis markers or backticks. Do not assume access to remote-session, screen, audio, files, prior conversation, or saved history."
            }
            Self::Star => {
                "When the question is behavioral, organize the answer as Situation, Task, Action, Result. Never invent the user's experience or missing facts; ask for the minimum missing detail when necessary. Do not assume access to remote-session, screen, audio, files, prior conversation, or saved history."
            }
        }
    }

    fn max_tokens(self) -> u32 {
        match self {
            Self::Default => 1_024,
            Self::Short => 384,
            Self::Star => 900,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskRequest {
    session_id: String,
    request_id: String,
    prompt: String,
    response_mode: ResponseMode,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AskIdentityRequest {
    session_id: String,
    request_id: String,
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    retry_after: Option<u64>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response =
            (self.status, Json(serde_json::json!({"error":self.code}))).into_response();
        if let Some(seconds) = self.retry_after {
            if let Ok(value) = seconds.to_string().parse() {
                response
                    .headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, value);
            }
        }
        response
    }
}

fn error(status: StatusCode, code: &'static str) -> ApiError {
    ApiError {
        status,
        code,
        retry_after: None,
    }
}

fn verified_subject_key(
    state: &IntegrationState,
    headers: &HeaderMap,
    uri: &axum::http::Uri,
    body: &[u8],
    now: i64,
) -> Result<String, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "delegation_required"))?;
    let claims = delegation::verify(
        token,
        &state.delegation,
        "POST",
        &uri.to_string(),
        body,
        now,
    )
    .map_err(|_| error(StatusCode::UNAUTHORIZED, "invalid_delegation"))?;
    Ok(store::subject_key(
        &claims.iss,
        &claims.aud,
        &claims.env,
        &claims.sub,
    ))
}

fn canonical_uuid(value: &str, code: &'static str) -> Result<(), ApiError> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| error(StatusCode::BAD_REQUEST, code))?;
    if id.is_nil() || id.hyphenated().to_string() != value {
        return Err(error(StatusCode::BAD_REQUEST, code));
    }
    Ok(())
}

async fn open_session(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<store::AiSession>, ApiError> {
    let now = Utc::now().timestamp();
    let key = verified_subject_key(&state, &headers, &uri, &body, now)?;
    let request: SessionRequest = serde_json::from_slice(&body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_session_request"))?;
    canonical_uuid(&request.session_id, "invalid_session_request")?;
    admission(&state, AdmissionClass::Start).await?;
    let entitlement = if state.synthetic_subject_keys.contains(&key) {
        state.synthetic_entitlement
    } else {
        None
    };
    let result = tokio::task::spawn_blocking(move || {
        store::open_with_entitlement(&state.pool, &key, &request.session_id, now, entitlement)
    })
    .await
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"))?
    .map_err(store_error)?;
    Ok(Json(result))
}

fn store_error(err: anyhow::Error) -> ApiError {
    match err.downcast_ref::<store::StoreError>() {
        Some(store::StoreError::Denied) => error(StatusCode::FORBIDDEN, "session_unavailable"),
        Some(store::StoreError::Capacity) => {
            error(StatusCode::TOO_MANY_REQUESTS, "session_capacity")
        }
        Some(store::StoreError::Cancelled) => error(StatusCode::CONFLICT, "request_unavailable"),
        None => error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"),
    }
}

async fn close_session(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<store::AiSession>, ApiError> {
    let now = Utc::now().timestamp();
    let key = verified_subject_key(&state, &headers, &uri, &body, now)?;
    let request: SessionRequest = serde_json::from_slice(&body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_session_request"))?;
    canonical_uuid(&request.session_id, "invalid_session_request")?;
    admission(&state, AdmissionClass::Stop).await?;
    let result = tokio::task::spawn_blocking(move || {
        store::close(&state.pool, &key, &request.session_id, now)
    })
    .await
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"))?
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "session_unavailable"))?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "session_unavailable"))?;
    Ok(Json(result))
}

fn validate_ask(request: &AskRequest) -> Result<(), ApiError> {
    canonical_uuid(&request.session_id, "invalid_ask_request")?;
    canonical_uuid(&request.request_id, "invalid_ask_request")?;
    if request.prompt.trim().is_empty()
        || request.prompt.len() > MAX_PROMPT_BYTES
        || request.prompt.chars().count() > MAX_PROMPT_CHARS
    {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_ask_request"));
    }
    Ok(())
}

fn validate_ask_identity(request: &AskIdentityRequest) -> Result<(), ApiError> {
    canonical_uuid(&request.session_id, "invalid_ask_request")?;
    canonical_uuid(&request.request_id, "invalid_ask_request")
}

fn answer_system(mode: ResponseMode) -> String {
    format!(
        "{}{}{} {}",
        cue_core::prompt_contracts::MANAGED_PROVIDER_BASE_CONTRACT,
        cue_core::prompt_contracts::MANAGED_PROVIDER_ANSWER_RULES_SEPARATOR,
        "Use a natural conversational voice: answer first, use clear everyday language and varied short sentences, and avoid canned openings, corporate filler, exaggerated praise or unnecessary headings. Sound like a thoughtful colleague, not a rewrite template. Preserve uncertainty and meaning rather than dressing guesses as facts. For interview answers, distinguish general examples from the user's actual experience; do not invent personal employers, responsibilities, dates, metrics or outcomes. Never claim to have read Otter transcripts or a private knowledge base unless authorized source context is actually supplied.",
        mode.answer_rules()
    )
}

fn final_presentation_rules(mode: ResponseMode) -> String {
    format!(
        "Use a natural conversational voice, answer first, avoid canned filler and unnecessary headings. Use plain text without Markdown emphasis markers. Never invent the user's experience or claim unavailable transcript access. {}",
        mode.answer_rules()
    )
}

fn status_event(status: &store::AiRequestStatus) -> Event {
    Event::default().event("status").data(
        serde_json::to_string(status)
            .unwrap_or_else(|_| r#"{"state":"failed","accounting_status":"pending"}"#.to_string()),
    )
}

struct ManagedRequestContext {
    pool: DbPool,
    key: String,
    session_id: String,
    request_id: String,
    may_finalize: bool,
}

fn managed_request_stream(
    mut source: crate::api::router::RouterSseStream,
    context: ManagedRequestContext,
    initial_status: store::AiRequestStatus,
    settlement_signal: ManagedSettlementSignal,
) -> crate::api::router::RouterSseStream {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut suppressed = !store::dispatch_allowed(
            &context.pool,
            &context.key,
            &context.session_id,
            &context.request_id,
            Utc::now().timestamp(),
        );
        let _ = sender.send(Ok(status_event(&initial_status)));
        while let Some(event) = source.next().await {
            if !suppressed
                && !store::dispatch_allowed(
                    &context.pool,
                    &context.key,
                    &context.session_id,
                    &context.request_id,
                    Utc::now().timestamp(),
                )
            {
                suppressed = true;
                if let Ok(Some(status)) = store::cancel_request(
                    &context.pool,
                    &context.key,
                    &context.session_id,
                    &context.request_id,
                    Utc::now().timestamp(),
                ) {
                    let _ = sender.send(Ok(status_event(&status)));
                }
            }
            if !suppressed {
                let _ = sender.send(event);
            }
        }
        if context.may_finalize {
            let accounting_settled = settlement_signal.is_terminal();
            if let Ok(Some(status)) = store::finish_request(
                &context.pool,
                &context.key,
                &context.session_id,
                &context.request_id,
                !accounting_settled,
                accounting_settled,
                Utc::now().timestamp(),
            ) {
                if suppressed {
                    let _ = sender.send(Ok(status_event(&status)));
                }
            }
        }
    });
    Box::pin(async_stream::stream! {
        while let Some(event) = receiver.recv().await {
            yield event;
        }
    })
}

async fn ask_stream(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    Extension(crate::api::middleware::request_id::TraceId(trace_id)): Extension<
        crate::api::middleware::request_id::TraceId,
    >,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Sse<crate::api::router::RouterSseStream>, ApiError> {
    let now = Utc::now().timestamp();
    let key = verified_subject_key(&state, &headers, &uri, &body, now)?;
    let request: AskRequest = serde_json::from_slice(&body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_ask_request"))?;
    validate_ask(&request)?;
    admission(&state, AdmissionClass::Ask).await?;

    let prompt_sha256 = hex::encode(Sha256::digest(request.prompt.as_bytes()));
    let response_mode = request.response_mode.as_str();
    let (account_id, status) = store::admit_ask(
        &state.pool,
        &key,
        &request.session_id,
        &request.request_id,
        &prompt_sha256,
        response_mode,
        now,
    )
    .map_err(store_error)?;
    let owns_execution = match status.state.as_str() {
        "admitted" => store::mark_request_running(
            &state.pool,
            &key,
            &request.session_id,
            &request.request_id,
            now,
        )
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "request_unavailable"))?,
        "running" => return Err(error(StatusCode::CONFLICT, "request_in_progress")),
        "finished" if status.accounting_status == "settled" => false,
        "cancellation_requested" | "cancelled" | "failed" => {
            return Err(error(StatusCode::CONFLICT, "request_unavailable"));
        }
        _ => return Err(error(StatusCode::CONFLICT, "request_unavailable")),
    };
    if status.state == "admitted" && !owns_execution {
        // Another exact replay won the durable admitted -> running claim.
        // It exclusively owns provider dispatch and final request mutation.
        return Err(error(StatusCode::CONFLICT, "request_in_progress"));
    }
    let running_status = store::status(&state.pool, &key, &request.session_id, &request.request_id)
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "request_unavailable"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "request_unavailable"))?;
    let account = crate::db::accounts::Account::fetch_by_id(&state.pool, &account_id)
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "request_unavailable"))?
        .ok_or_else(|| error(StatusCode::FORBIDDEN, "request_unavailable"))?;

    let dispatch_pool = state.pool.clone();
    let dispatch_key = key.clone();
    let dispatch_session = request.session_id.clone();
    let dispatch_request = request.request_id.clone();
    let fence = Arc::new(move || {
        store::dispatch_allowed(
            &dispatch_pool,
            &dispatch_key,
            &dispatch_session,
            &dispatch_request,
            Utc::now().timestamp(),
        )
    });
    let settlement_signal = ManagedSettlementSignal::default();
    let system = answer_system(request.response_mode);
    let estimated_input_tokens =
        crate::pricing::utf8_input_token_upper_bound([system.as_str(), request.prompt.as_str()]);
    let managed_request = CompleteRequest {
        request_id: request.request_id.clone(),
        system,
        user: request.prompt,
        session_id: None,
        max_tokens: Some(request.response_mode.max_tokens()),
        temperature: None,
        reasoning_effort: None,
        thinking_budget_tokens: None,
        lane: "instant".into(),
        estimated_input_tokens: Some(estimated_input_tokens),
        image_data_urls: Vec::new(),
        context_schema_version: None,
        context: Vec::new(),
    };
    let source = match crate::api::router::complete_stream_for_account(
        state.managed.clone(),
        account,
        managed_request,
        trace_id,
        ManagedStreamPolicy::delegated_text(
            fence,
            settlement_signal.clone(),
            final_presentation_rules(request.response_mode),
        ),
    )
    .await
    {
        Ok(source) => source,
        Err((status_code, _)) => {
            if owns_execution {
                let accounting_settled = settlement_signal.is_terminal();
                let _ = store::finish_request(
                    &state.pool,
                    &key,
                    &request.session_id,
                    &request.request_id,
                    true,
                    accounting_settled,
                    Utc::now().timestamp(),
                );
            }
            return Err(error(status_code, "managed_request_unavailable"));
        }
    };
    let stream = managed_request_stream(
        source,
        ManagedRequestContext {
            pool: state.pool.clone(),
            key,
            session_id: request.session_id,
            request_id: request.request_id,
            may_finalize: owns_execution,
        },
        running_status,
        settlement_signal,
    );
    Ok(crate::api::router::router_sse(stream))
}

async fn cancel_ask(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<store::AiRequestStatus>, ApiError> {
    let now = Utc::now().timestamp();
    let key = verified_subject_key(&state, &headers, &uri, &body, now)?;
    let request: AskIdentityRequest = serde_json::from_slice(&body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_ask_request"))?;
    validate_ask_identity(&request)?;
    admission(&state, AdmissionClass::Stop).await?;
    let status = store::cancel_request(
        &state.pool,
        &key,
        &request.session_id,
        &request.request_id,
        now,
    )
    .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "request_unavailable"))?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "request_unavailable"))?;
    Ok(Json(status))
}

async fn ask_status(
    State(state): State<IntegrationState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<store::AiRequestStatus>, ApiError> {
    let now = Utc::now().timestamp();
    let key = verified_subject_key(&state, &headers, &uri, &body, now)?;
    let request: AskIdentityRequest = serde_json::from_slice(&body)
        .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_ask_request"))?;
    validate_ask_identity(&request)?;
    admission(&state, AdmissionClass::Status).await?;
    let status = store::status(&state.pool, &key, &request.session_id, &request.request_id)
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "request_unavailable"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "request_unavailable"))?;
    Ok(Json(status))
}
