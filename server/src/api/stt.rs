//! STT authorization endpoint.
//!
//! The server never returns static provider API keys. The alpha response is a
//! Bluey-scoped session token for a server relay path. If a provider later
//! supports safe short-lived scoped credentials, the same response shape can
//! switch `mode` to `provider_direct` and include only that ephemeral token.

use axum::{
    extract::{
        ws::{Message as ClientMessage, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::HeaderValue, Message as UpstreamMessage,
};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::{
    accounts::Account,
    stt_accounting::{
        self, ClaimSttSessionError, ClaimedSttSession, ReserveSessionInput, SttAccountingError,
    },
    usage,
};
use crate::pricing;

const DEFAULT_MAX_SECONDS: i64 = 10 * 60;
const MAX_SESSION_SECONDS: i64 = 20 * 60;
const PCM16_DBFS_FLOOR: f64 = -120.0;
const LIVE_STT_AUDIBLE_RMS_DBFS: f64 = -58.0;
const LIVE_STT_AUDIBLE_PEAK_DBFS: f64 = -34.0;
const DEFAULT_DEEPGRAM_ENDPOINTING_MS: u32 = 10;
const DEFAULT_DEEPGRAM_UTTERANCE_END_MS: Option<u32> = None;
const DEFAULT_DEEPGRAM_LANGUAGE: &str = "en-IN";
const DEFAULT_DEEPGRAM_NO_DELAY: bool = true;
const DEFAULT_DEEPGRAM_SMART_FORMAT: bool = false;
const MAX_DEEPGRAM_KEYTERMS: usize = 64;
const DEFAULT_DEEPGRAM_KEYTERMS: &[&str] = &[
    "LRU",
    "FIFO",
    "LIFO",
    "API",
    "REST API",
    "GraphQL",
    "gRPC",
    "SQL",
    "NoSQL",
    "PostgreSQL",
    "MySQL",
    "Redis",
    "Kafka",
    "Kubernetes",
    "Docker",
    "Terraform",
    "CI/CD",
    "AWS",
    "Azure",
    "GCP",
    "OAuth",
    "JWT",
    "Python",
    "Java",
    "JavaScript",
    "TypeScript",
    "C#",
    "React",
    "Angular",
    "LangGraph",
    "Deepgram",
    "OpenAI",
    "Claude",
    "Gemini",
    "Bluey",
    "LeetCode",
    "Fibonacci",
    "Two Sum",
    "given a set",
    "set of two numbers",
    "set of numbers",
    "array of integers",
    "single digit",
    "double digit",
    "hash map",
    "linked list",
    "binary search",
    "sliding window",
    "monotonic stack",
    "dynamic programming",
];

#[derive(Debug, Deserialize)]
pub struct SttSessionRequest {
    pub session_id: String,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub requested_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SttSessionResponse {
    pub mode: String,
    pub provider: String,
    pub model: String,
    pub session_token: String,
    pub expires_at_ms: i64,
    pub max_seconds: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub websocket_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SttSessionCancelRequest {
    pub session_token: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SttSessionCancelResponse {
    pub released: bool,
}

#[derive(Debug, Deserialize)]
pub struct SttRelayQuery {
    pub session_token: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<SttSessionRequest>,
) -> Result<Json<SttSessionResponse>, (StatusCode, String)> {
    if account.billing_restricted {
        return Err((
            StatusCode::FORBIDDEN,
            "Account usage is paused while billing is under review.".into(),
        ));
    }
    if req.session_id.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_id is required".into()));
    }
    let provider = req.provider.unwrap_or_else(|| "deepgram".into());
    let model = req.model.unwrap_or_else(|| "nova-3".into());
    if provider != "deepgram" {
        return Err((
            StatusCode::BAD_REQUEST,
            "only deepgram STT sessions are enabled for alpha".into(),
        ));
    }
    let max_seconds = req
        .requested_seconds
        .unwrap_or(DEFAULT_MAX_SECONDS)
        .clamp(30, MAX_SESSION_SECONDS);

    let estimated_bluey_cost_cents = estimate_deepgram_bluey_cost_cents(&model, max_seconds)?;
    check_upstream_spend_guard(
        &state,
        &account.id,
        estimated_bluey_cost_cents,
        "stt_session",
    )?;

    let token = random_token();
    let now = now_ms();
    let expires_at = now + (max_seconds * 1000);
    let mode = "server_relay";
    let reservation = stt_accounting::reserve_session(
        &state.pool,
        ReserveSessionInput {
            account_id: &account.id,
            bluey_session_id: &req.session_id,
            provider: &provider,
            model: &model,
            source: &req.source,
            mode,
            token: &token,
            max_seconds,
            created_at_ms: now,
            expires_at_ms: expires_at,
        },
    )
    .map_err(map_create_error)?;

    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
        source = %req.source,
        provider = %provider,
        model = %model,
        reserved_cents = reservation.reserved_cents,
        reserved_trial_seconds = reservation.reserved_trial_seconds,
        reserved_billable_seconds = reservation.reserved_billable_seconds,
        projected_bluey_cents = reservation.projected_bluey_cents,
        "STT relay session reserved"
    );

    Ok(Json(SttSessionResponse {
        mode: mode.to_string(),
        provider,
        model,
        session_token: token,
        expires_at_ms: expires_at,
        max_seconds,
        websocket_url: Some(format!(
            "{}/stt/relay",
            state.config.public_url.trim_end_matches('/')
        )),
        provider_token: None,
    }))
}

pub async fn cancel_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<SttSessionCancelRequest>,
) -> Result<Json<SttSessionCancelResponse>, (StatusCode, String)> {
    let token = req.session_token.trim();
    if token.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_token is required".into()));
    }
    let model = req.model.unwrap_or_else(|| "nova-3".to_string());
    let reason = req
        .reason
        .filter(|reason| !reason.trim().is_empty())
        .unwrap_or_else(|| "client_cancelled_before_audio".to_string());
    match stt_accounting::settle_session(
        &state.pool,
        token,
        &account.id,
        &model,
        0,
        &reason,
        now_ms(),
    ) {
        Ok(settled) => {
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                refunded_cents = settled.refunded_cents,
                refunded_trial_seconds = settled.refunded_trial_seconds,
                reason = %reason,
                "STT relay session canceled and reservation released"
            );
            Ok(Json(SttSessionCancelResponse { released: true }))
        }
        Err(SttAccountingError::AlreadySettled) => {
            Ok(Json(SttSessionCancelResponse { released: false }))
        }
        Err(error) => Err(map_create_error(error)),
    }
}

pub async fn relay(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Query(query): Query<SttRelayQuery>,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    let session = claim_relay_session(&state, &account.id, &query.session_token)?;
    let deepgram_key = state
        .config
        .upstream
        .deepgram_key(&session.token)
        .map(str::to_string)
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "STT relay is not configured".to_string(),
            )
        })?;
    Ok(ws
        .on_upgrade(move |socket| async move {
            if let Err(err) = run_deepgram_relay(socket, state, session, deepgram_key).await {
                tracing::warn!(error = %err, "STT relay closed with error");
            }
        })
        .into_response())
}

fn default_source() -> String {
    "microphone".to_string()
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    tracing::warn!(error = %e, "stt session endpoint failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "stt session failed".to_string(),
    )
}

fn map_create_error(error: SttAccountingError) -> (StatusCode, String) {
    match error {
        SttAccountingError::UnsupportedModel => (
            StatusCode::BAD_REQUEST,
            "unsupported Deepgram STT model".to_string(),
        ),
        SttAccountingError::InsufficientBalance => (
            StatusCode::PAYMENT_REQUIRED,
            "balance is required before starting this STT session".to_string(),
        ),
        SttAccountingError::AlreadySettled => (
            StatusCode::CONFLICT,
            "STT session is already closed".to_string(),
        ),
        SttAccountingError::Db(err) => {
            tracing::warn!(error = %err, "STT accounting failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "stt session failed".to_string(),
            )
        }
    }
}

fn map_claim_error(error: ClaimSttSessionError) -> (StatusCode, String) {
    match error {
        ClaimSttSessionError::InvalidSession => {
            (StatusCode::UNAUTHORIZED, "invalid STT session".to_string())
        }
        ClaimSttSessionError::Expired => (StatusCode::GONE, "STT session expired".to_string()),
        ClaimSttSessionError::UnsupportedProvider => (
            StatusCode::BAD_REQUEST,
            "only deepgram STT relay is enabled".to_string(),
        ),
        ClaimSttSessionError::AlreadyActiveOrClosed => (
            StatusCode::CONFLICT,
            "STT session is already active or closed".to_string(),
        ),
        ClaimSttSessionError::Db(err) => internal(err),
    }
}

#[cfg(test)]
fn estimate_deepgram_cost_cents(model: &str, seconds: i64) -> Result<i64, (StatusCode, String)> {
    if seconds <= 0 {
        return Ok(0);
    }
    let pricing = pricing::lookup("deepgram", model).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "unsupported Deepgram STT model".to_string(),
        )
    })?;
    let (_, customer_cents) = pricing::compute_cost(pricing, seconds, 0);
    Ok(customer_cents)
}

fn estimate_deepgram_bluey_cost_cents(
    model: &str,
    seconds: i64,
) -> Result<i64, (StatusCode, String)> {
    if seconds <= 0 {
        return Ok(0);
    }
    let pricing = pricing::lookup("deepgram", model).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "unsupported Deepgram STT model".to_string(),
        )
    })?;
    let (bluey_cents, _) = pricing::compute_cost(pricing, seconds, 0);
    Ok(bluey_cents + (bluey_cents / 10).max(1))
}

fn check_upstream_spend_guard(
    state: &AppState,
    account_id: &str,
    projected_bluey_cents: i64,
    kind: &str,
) -> Result<(), (StatusCode, String)> {
    let Some(guard) = state.config.upstream_spend_guard else {
        return Ok(());
    };
    if projected_bluey_cents <= 0 {
        return Ok(());
    }
    let current =
        usage::bluey_spend_cents_in_window(&state.pool, guard.window_hours).map_err(internal)?;
    if current.saturating_add(projected_bluey_cents) > guard.limit_cents {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            kind,
            current_bluey_cents = current,
            projected_bluey_cents,
            limit_bluey_cents = guard.limit_cents,
            window_hours = guard.window_hours,
            "upstream spend guard paused STT session creation"
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Bluey live-test budget is paused; operator action required".to_string(),
        ));
    }
    Ok(())
}

fn claim_relay_session(
    state: &AppState,
    account_id: &str,
    token: &str,
) -> Result<ClaimedSttSession, (StatusCode, String)> {
    if token.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_token is required".into()));
    }
    stt_accounting::claim_relay_session(&state.pool, account_id, token, now_ms())
        .map_err(map_claim_error)
}

async fn run_deepgram_relay(
    socket: WebSocket,
    state: AppState,
    session: ClaimedSttSession,
    deepgram_key: String,
) -> anyhow::Result<()> {
    let started = Instant::now();
    let url = deepgram_realtime_url(&session);
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Token {deepgram_key}"))?,
    );

    let (upstream, _) = tokio_tungstenite::connect_async(request).await?;
    let (mut client_tx, mut client_rx) = socket.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    let deadline = Duration::from_secs(session.max_seconds.max(1) as u64);
    let mut close_reason = "completed".to_string();
    let mut forwarded_audio_bytes = 0_u64;
    let mut forwarded_audio_chunks = 0_u64;
    let mut forwarded_audible_audio_chunks = 0_u64;
    let mut first_audible_after_ms: Option<u128> = None;
    let mut provider_frame_stats = DeepgramRelayFrameStats::default();

    tokio::select! {
        _ = tokio::time::sleep(deadline) => {
            close_reason = "max_seconds".to_string();
            let _ = client_tx.send(ClientMessage::Text("{\"type\":\"bluey.stt_session_limit\"}".into())).await;
            let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
        }
        result = async {
            while let Some(message) = client_rx.next().await {
                ensure_stt_account_active(&state, &session.account_id)?;
                match message? {
                    ClientMessage::Binary(bytes) => {
                        if !bytes.is_empty() {
                            forwarded_audio_bytes = forwarded_audio_bytes.saturating_add(bytes.len() as u64);
                            forwarded_audio_chunks = forwarded_audio_chunks.saturating_add(1);
                            let stats = pcm16_i16le_stats(bytes.as_ref());
                            if stats.is_audible_for_stt() {
                                forwarded_audible_audio_chunks =
                                    forwarded_audible_audio_chunks.saturating_add(1);
                                if first_audible_after_ms.is_none() {
                                    first_audible_after_ms = Some(started.elapsed().as_millis());
                                }
                            }
                            if forwarded_audio_chunks == 1
                                || forwarded_audio_chunks.is_multiple_of(50)
                            {
                                tracing::info!(
                                    account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
                                    source = %session.source,
                                    provider = %session.provider,
                                    model = %session.model,
                                    audio_chunks = forwarded_audio_chunks,
                                    audible_audio_chunks = forwarded_audible_audio_chunks,
                                    first_audible_after_ms = first_audible_after_ms.unwrap_or(0),
                                    audio_bytes = forwarded_audio_bytes,
                                    chunk_bytes = bytes.len(),
                                    samples = stats.samples,
                                    rms_dbfs = stats.rms_dbfs,
                                    peak_dbfs = stats.peak_dbfs,
                                    nonzero_percent = stats.nonzero_percent,
                                    "STT relay forwarded audio level"
                                );
                            }
                        }
                        upstream_tx.send(UpstreamMessage::Binary(bytes)).await?
                    }
                    ClientMessage::Text(text) => upstream_tx.send(UpstreamMessage::Text(text)).await?,
                    ClientMessage::Ping(bytes) => upstream_tx.send(UpstreamMessage::Ping(bytes)).await?,
                    ClientMessage::Pong(bytes) => upstream_tx.send(UpstreamMessage::Pong(bytes)).await?,
                    ClientMessage::Close(frame) => {
                        upstream_tx.send(UpstreamMessage::Close(frame.map(|f| {
                            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                code: f.code.into(),
                                reason: f.reason.to_string().into(),
                            }
                        }))).await?;
                        break;
                    }
                }
            }
            anyhow::Ok(())
        } => {
            if let Err(err) = result {
                if is_account_closed_error(&err) {
                    close_reason = "account_deleted".to_string();
                    let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
                } else {
                    close_reason = "client_to_provider_error".to_string();
                    tracing::warn!(error = %err, "STT relay client-to-provider pipe failed");
                }
            }
        }
        result = async {
            while let Some(message) = upstream_rx.next().await {
                ensure_stt_account_active(&state, &session.account_id)?;
                match message? {
                    UpstreamMessage::Text(text) => {
                        provider_frame_stats.observe(&session, started, &text);
                        client_tx.send(ClientMessage::Text(text)).await?
                    }
                    UpstreamMessage::Binary(bytes) => client_tx.send(ClientMessage::Binary(bytes)).await?,
                    UpstreamMessage::Ping(bytes) => client_tx.send(ClientMessage::Ping(bytes)).await?,
                    UpstreamMessage::Pong(bytes) => client_tx.send(ClientMessage::Pong(bytes)).await?,
                    UpstreamMessage::Close(_) => {
                        let _ = client_tx.send(ClientMessage::Close(None)).await;
                        break;
                    }
                    UpstreamMessage::Frame(_) => {}
                }
            }
            anyhow::Ok(())
        } => {
            if let Err(err) = result {
                if is_account_closed_error(&err) {
                    close_reason = "account_deleted".to_string();
                    let _ = client_tx.send(ClientMessage::Close(None)).await;
                } else {
                    close_reason = "provider_to_client_error".to_string();
                    tracing::warn!(error = %err, "STT relay provider-to-client pipe failed");
                }
            }
        }
    }

    if close_reason == "account_deleted" {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
            source = %session.source,
            provider = %session.provider,
            model = %session.model,
            forwarded_audio_bytes,
            forwarded_audio_chunks,
            forwarded_audible_audio_chunks,
            "STT relay stopped because account was deleted; skipping billing settlement"
        );
        return Ok(());
    }

    let billable_elapsed = if forwarded_audible_audio_chunks == 0 {
        Duration::ZERO
    } else {
        started.elapsed()
    };
    let settle_reason = if forwarded_audible_audio_chunks == 0 {
        format!("{close_reason}:no_audible_audio")
    } else {
        close_reason
    };
    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
        source = %session.source,
        provider = %session.provider,
        model = %session.model,
        forwarded_audio_bytes,
        forwarded_audio_chunks,
        forwarded_audible_audio_chunks,
        provider_text_frames = provider_frame_stats.text_frames,
        provider_transcript_frames = provider_frame_stats.transcript_frames,
        provider_partial_frames = provider_frame_stats.partial_frames,
        provider_final_frames = provider_frame_stats.final_frames,
        provider_empty_transcript_frames = provider_frame_stats.empty_transcript_frames,
        provider_control_frames = provider_frame_stats.control_frames,
        first_provider_frame_after_ms = provider_frame_stats.first_text_after_ms.unwrap_or(0),
        first_provider_transcript_after_ms = provider_frame_stats
            .first_transcript_after_ms
            .unwrap_or(0),
        first_provider_partial_after_ms = provider_frame_stats.first_partial_after_ms.unwrap_or(0),
        first_provider_final_after_ms = provider_frame_stats.first_final_after_ms.unwrap_or(0),
        billable_elapsed_ms = billable_elapsed.as_millis() as u64,
        settle_reason = %settle_reason,
        "STT relay settlement prepared"
    );
    finalize_relay_session(
        &state,
        &session,
        billable_elapsed,
        &settle_reason,
        forwarded_audio_bytes,
        forwarded_audio_chunks,
    )?;
    Ok(())
}

fn ensure_stt_account_active(state: &AppState, account_id: &str) -> anyhow::Result<()> {
    match Account::fetch_by_id(&state.pool, account_id) {
        Ok(Some(account)) if !account.billing_restricted => Ok(()),
        Ok(Some(_)) => anyhow::bail!("account_closed"),
        Ok(None) => anyhow::bail!("account_closed"),
        Err(e) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                error = %e,
                "failed to verify STT account liveness"
            );
            anyhow::bail!("account_closed")
        }
    }
}

fn is_account_closed_error(error: &anyhow::Error) -> bool {
    error.to_string().contains("account_closed")
}

fn deepgram_realtime_url(session: &ClaimedSttSession) -> String {
    let base = std::env::var("BLUEY_TEST_DEEPGRAM_WS_URL")
        .unwrap_or_else(|_| "wss://api.deepgram.com/v1/listen".to_string());
    let endpointing_ms = deepgram_realtime_env_u32(
        "BLUEY_DEEPGRAM_ENDPOINTING_MS",
        DEFAULT_DEEPGRAM_ENDPOINTING_MS,
        10,
        1_000,
    );
    let utterance_end_ms = deepgram_realtime_env_optional_u32(
        "BLUEY_DEEPGRAM_UTTERANCE_END_MS",
        DEFAULT_DEEPGRAM_UTTERANCE_END_MS,
        1_000,
        5_000,
    );
    let no_delay = deepgram_realtime_env_bool("BLUEY_DEEPGRAM_NO_DELAY", DEFAULT_DEEPGRAM_NO_DELAY);
    let smart_format =
        deepgram_realtime_env_bool("BLUEY_DEEPGRAM_SMART_FORMAT", DEFAULT_DEEPGRAM_SMART_FORMAT);
    let keyterms = deepgram_realtime_keyterms();
    let mut url = format!(
        "{base}?model={}&encoding=linear16&sample_rate=16000&channels=1&punctuate=true&smart_format={smart_format}&interim_results=true&endpointing={endpointing_ms}&vad_events=true&no_delay={no_delay}",
        url_escape(&session.model),
    );
    if let Some(utterance_end_ms) = utterance_end_ms {
        url.push_str("&utterance_end_ms=");
        url.push_str(&utterance_end_ms.to_string());
    }
    let language = std::env::var("BLUEY_DEEPGRAM_LANGUAGE")
        .unwrap_or_else(|_| DEFAULT_DEEPGRAM_LANGUAGE.to_string());
    let language = language.trim();
    if !language.is_empty()
        && !matches!(
            language.to_ascii_lowercase().as_str(),
            "auto" | "detect" | "none" | "off"
        )
    {
        url.push_str("&language=");
        url.push_str(&url_escape(language));
    }
    for keyterm in keyterms {
        url.push_str("&keyterm=");
        url.push_str(&url_escape(&keyterm));
    }
    url
}

fn deepgram_realtime_keyterms() -> Vec<String> {
    let include_defaults = deepgram_realtime_env_bool("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS", true);
    let mut keyterms = Vec::new();
    if include_defaults {
        for keyterm in DEFAULT_DEEPGRAM_KEYTERMS {
            push_deepgram_keyterm(&mut keyterms, keyterm);
        }
    }
    if let Ok(raw) = std::env::var("BLUEY_DEEPGRAM_KEYTERMS") {
        let disabled = matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off" | "none"
        );
        if !disabled {
            for keyterm in raw.split([',', ';', '\n']) {
                push_deepgram_keyterm(&mut keyterms, keyterm);
            }
        }
    }
    keyterms
}

fn push_deepgram_keyterm(keyterms: &mut Vec<String>, keyterm: &str) {
    if keyterms.len() >= MAX_DEEPGRAM_KEYTERMS {
        return;
    }
    let keyterm = keyterm.trim();
    if keyterm.is_empty() {
        return;
    }
    if keyterms
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(keyterm))
    {
        return;
    }
    keyterms.push(keyterm.to_string());
}

fn deepgram_realtime_env_u32(name: &str, default: u32, min: u32, max: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

fn deepgram_realtime_env_optional_u32(
    name: &str,
    default: Option<u32>,
    min: u32,
    max: u32,
) -> Option<u32> {
    let Some(value) = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return default;
    };
    if matches!(
        value.to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "none" | "disabled" | "disable"
    ) {
        return None;
    }
    value
        .parse::<u32>()
        .ok()
        .map(|value| value.clamp(min, max))
        .or(default)
}

fn deepgram_realtime_env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            let value = value.trim();
            matches!(value, "1")
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on")
        })
        .unwrap_or(default)
}

#[derive(Debug, Default)]
struct DeepgramRelayFrameStats {
    text_frames: u64,
    transcript_frames: u64,
    partial_frames: u64,
    final_frames: u64,
    empty_transcript_frames: u64,
    control_frames: u64,
    first_text_after_ms: Option<u64>,
    first_transcript_after_ms: Option<u64>,
    first_partial_after_ms: Option<u64>,
    first_final_after_ms: Option<u64>,
}

impl DeepgramRelayFrameStats {
    fn observe(&mut self, session: &ClaimedSttSession, started: Instant, payload: &str) {
        self.text_frames = self.text_frames.saturating_add(1);
        let elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.first_text_after_ms.get_or_insert(elapsed_ms);

        let inspection = inspect_deepgram_relay_frame(payload);
        if inspection.is_transcript_frame {
            if inspection.transcript_chars == 0 {
                self.empty_transcript_frames = self.empty_transcript_frames.saturating_add(1);
            } else {
                self.transcript_frames = self.transcript_frames.saturating_add(1);
                self.first_transcript_after_ms.get_or_insert(elapsed_ms);
                if inspection.is_final {
                    self.final_frames = self.final_frames.saturating_add(1);
                    self.first_final_after_ms.get_or_insert(elapsed_ms);
                } else {
                    self.partial_frames = self.partial_frames.saturating_add(1);
                    self.first_partial_after_ms.get_or_insert(elapsed_ms);
                }
                if self.transcript_frames == 1
                    || self.transcript_frames == 5
                    || self.transcript_frames.is_multiple_of(25)
                    || inspection.is_final
                {
                    tracing::info!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
                        source = %session.source,
                        provider = %session.provider,
                        model = %session.model,
                        frame_type = %inspection.frame_type,
                        is_final = inspection.is_final,
                        speech_final = inspection.speech_final,
                        transcript_chars = inspection.transcript_chars,
                        transcript_words = inspection.transcript_words,
                        provider_elapsed_ms = elapsed_ms,
                        provider_transcript_frames = self.transcript_frames,
                        provider_partial_frames = self.partial_frames,
                        provider_final_frames = self.final_frames,
                        "STT relay provider transcript frame"
                    );
                }
            }
        } else {
            self.control_frames = self.control_frames.saturating_add(1);
        }
    }
}

#[derive(Debug, Default)]
struct DeepgramRelayFrameInspection {
    frame_type: String,
    is_transcript_frame: bool,
    is_final: bool,
    speech_final: bool,
    transcript_chars: usize,
    transcript_words: usize,
}

fn inspect_deepgram_relay_frame(payload: &str) -> DeepgramRelayFrameInspection {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return DeepgramRelayFrameInspection {
            frame_type: "invalid_json".to_string(),
            ..DeepgramRelayFrameInspection::default()
        };
    };
    let frame_type = value
        .get("type")
        .and_then(|ty| ty.as_str())
        .unwrap_or("unknown")
        .to_string();
    let is_transcript_frame = matches!(frame_type.as_str(), "Results" | "unknown")
        && value
            .get("channel")
            .and_then(|channel| channel.as_object())
            .is_some();
    let transcript = value
        .get("channel")
        .and_then(|channel| channel.get("alternatives"))
        .and_then(|alternatives| alternatives.as_array())
        .and_then(|alternatives| alternatives.first())
        .and_then(|alternative| alternative.get("transcript"))
        .and_then(|transcript| transcript.as_str())
        .unwrap_or("")
        .trim();
    DeepgramRelayFrameInspection {
        frame_type,
        is_transcript_frame,
        is_final: value
            .get("is_final")
            .and_then(|is_final| is_final.as_bool())
            .unwrap_or(false),
        speech_final: value
            .get("speech_final")
            .and_then(|speech_final| speech_final.as_bool())
            .unwrap_or(false),
        transcript_chars: transcript.chars().count(),
        transcript_words: transcript.split_whitespace().count(),
    }
}

fn url_escape(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Pcm16AudioStats {
    samples: usize,
    rms_dbfs: f64,
    peak_dbfs: f64,
    nonzero_percent: f64,
}

impl Pcm16AudioStats {
    fn is_audible_for_stt(self) -> bool {
        self.samples > 0
            && (self.rms_dbfs >= LIVE_STT_AUDIBLE_RMS_DBFS
                || self.peak_dbfs >= LIVE_STT_AUDIBLE_PEAK_DBFS)
    }
}

fn pcm16_i16le_stats(raw: &[u8]) -> Pcm16AudioStats {
    let sample_bytes = raw.len() - (raw.len() % 2);
    if sample_bytes == 0 {
        return Pcm16AudioStats {
            samples: 0,
            rms_dbfs: PCM16_DBFS_FLOOR,
            peak_dbfs: PCM16_DBFS_FLOOR,
            nonzero_percent: 0.0,
        };
    }

    let mut samples = 0_usize;
    let mut peak = 0_i32;
    let mut nonzero = 0_usize;
    let mut sum_squares = 0_f64;
    for chunk in raw[..sample_bytes].chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
        let magnitude = sample.abs();
        if magnitude > 0 {
            nonzero = nonzero.saturating_add(1);
        }
        peak = peak.max(magnitude);
        sum_squares += (sample as f64) * (sample as f64);
        samples = samples.saturating_add(1);
    }

    let rms = if samples == 0 {
        0.0
    } else {
        (sum_squares / samples as f64).sqrt()
    };
    Pcm16AudioStats {
        samples,
        rms_dbfs: pcm16_dbfs(rms),
        peak_dbfs: pcm16_dbfs(peak as f64),
        nonzero_percent: (nonzero as f64 * 100.0) / samples.max(1) as f64,
    }
}

fn pcm16_dbfs(magnitude: f64) -> f64 {
    if magnitude <= 0.0 {
        PCM16_DBFS_FLOOR
    } else {
        (20.0 * (magnitude / 32768.0).log10()).max(PCM16_DBFS_FLOOR)
    }
}

fn finalize_relay_session(
    state: &AppState,
    session: &ClaimedSttSession,
    elapsed: Duration,
    reason: &str,
    audio_bytes: u64,
    audio_chunks: u64,
) -> anyhow::Result<()> {
    let elapsed_ms = elapsed.as_millis().min(i64::MAX as u128) as i64;
    let settled = stt_accounting::settle_session(
        &state.pool,
        &session.token,
        &session.account_id,
        &session.model,
        elapsed_ms,
        reason,
        now_ms(),
    )?;
    usage::record(
        &state.pool,
        &session.account_id,
        &usage::UsageEvent {
            request_id: format!("stt-{}-{}", session.bluey_session_id, session.token),
            kind: "stt".to_string(),
            task_type: Some("transcription".to_string()),
            lane: Some(session.source.clone()),
            provider: Some(session.provider.clone()),
            model: Some(session.model.clone()),
            input_tokens: settled.elapsed_seconds,
            output_tokens: 0,
            latency_ms: settled.elapsed_ms,
            cost_cents_to_bluey: settled.bluey_cents,
            cost_cents_to_customer: settled.customer_cents,
            was_speculative: false,
            was_fallback: false,
        },
    )?;
    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
        source = %session.source,
        provider = %session.provider,
        model = %session.model,
        elapsed_seconds = settled.elapsed_seconds,
        billable_seconds = settled.billable_seconds,
        trial_seconds = settled.trial_seconds,
        reserved_cents = session.reserved_cents,
        reserved_trial_seconds = session.reserved_trial_seconds,
        cost_cents = settled.customer_cents,
        refunded_cents = settled.refunded_cents,
        refunded_trial_seconds = settled.refunded_trial_seconds,
        audio_bytes,
        audio_chunks,
        reason,
        "STT relay session settled"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static DEEPGRAM_URL_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn random_token_is_url_safe_and_long() {
        let token = random_token();
        assert!(token.len() >= 40);
        assert!(!token.contains('+'));
        assert!(!token.contains('/'));
    }

    #[test]
    fn deepgram_url_escapes_model() {
        let _guard = DEEPGRAM_URL_ENV_LOCK.lock().unwrap();
        std::env::remove_var("BLUEY_DEEPGRAM_LANGUAGE");
        std::env::remove_var("BLUEY_DEEPGRAM_NO_DELAY");
        std::env::remove_var("BLUEY_DEEPGRAM_SMART_FORMAT");
        std::env::remove_var("BLUEY_DEEPGRAM_ENDPOINTING_MS");
        std::env::remove_var("BLUEY_DEEPGRAM_UTTERANCE_END_MS");
        std::env::remove_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS");
        std::env::remove_var("BLUEY_DEEPGRAM_KEYTERMS");
        let session = ClaimedSttSession {
            token: "token".into(),
            account_id: "acct".into(),
            bluey_session_id: "sess".into(),
            provider: "deepgram".into(),
            model: "nova 3/test".into(),
            source: "microphone".into(),
            max_seconds: 60,
            expires_at_ms: now_ms() + 60_000,
            reserved_cents: 0,
            reserved_trial_seconds: 0,
        };
        let url = deepgram_realtime_url(&session);
        assert!(url.contains("model=nova%203%2Ftest"));
        assert!(url.contains("interim_results=true"));
        assert!(url.contains("endpointing=10"));
        assert!(url.contains("smart_format=false"));
        assert!(!url.contains("utterance_end_ms="));
        assert!(url.contains("vad_events=true"));
        assert!(url.contains("no_delay=true"));
        assert!(url.contains("language=en-IN"));
        assert!(url.contains("keyterm=LRU"));
        assert!(url.contains("keyterm=REST%20API"));
        assert!(url.contains("keyterm=CI%2FCD"));
        assert!(!url.contains("Token "));
    }

    #[test]
    fn deepgram_url_can_enable_readability_preset() {
        let _guard = DEEPGRAM_URL_ENV_LOCK.lock().unwrap();
        std::env::set_var("BLUEY_DEEPGRAM_SMART_FORMAT", "true");
        std::env::set_var("BLUEY_DEEPGRAM_ENDPOINTING_MS", "150");
        std::env::set_var("BLUEY_DEEPGRAM_UTTERANCE_END_MS", "1000");
        std::env::set_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS", "0");
        std::env::remove_var("BLUEY_DEEPGRAM_LANGUAGE");
        std::env::remove_var("BLUEY_DEEPGRAM_KEYTERMS");
        let session = ClaimedSttSession {
            token: "token".into(),
            account_id: "acct".into(),
            bluey_session_id: "sess".into(),
            provider: "deepgram".into(),
            model: "nova-3".into(),
            source: "microphone".into(),
            max_seconds: 60,
            expires_at_ms: now_ms() + 60_000,
            reserved_cents: 0,
            reserved_trial_seconds: 0,
        };
        let url = deepgram_realtime_url(&session);
        assert!(url.contains("endpointing=150"));
        assert!(url.contains("smart_format=true"));
        assert!(url.contains("utterance_end_ms=1000"));
        assert!(url.contains("language=en-IN"));
        std::env::remove_var("BLUEY_DEEPGRAM_SMART_FORMAT");
        std::env::remove_var("BLUEY_DEEPGRAM_ENDPOINTING_MS");
        std::env::remove_var("BLUEY_DEEPGRAM_UTTERANCE_END_MS");
        std::env::remove_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS");
        std::env::remove_var("BLUEY_DEEPGRAM_LANGUAGE");
    }

    #[test]
    fn deepgram_url_can_omit_default_language() {
        let _guard = DEEPGRAM_URL_ENV_LOCK.lock().unwrap();
        std::env::set_var("BLUEY_DEEPGRAM_LANGUAGE", "auto");
        std::env::set_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS", "0");
        std::env::remove_var("BLUEY_DEEPGRAM_KEYTERMS");
        let session = ClaimedSttSession {
            token: "token".into(),
            account_id: "acct".into(),
            bluey_session_id: "sess".into(),
            provider: "deepgram".into(),
            model: "nova-3".into(),
            source: "microphone".into(),
            max_seconds: 60,
            expires_at_ms: now_ms() + 60_000,
            reserved_cents: 0,
            reserved_trial_seconds: 0,
        };
        let url = deepgram_realtime_url(&session);
        assert!(!url.contains("language="));
        assert!(!url.contains("keyterm="));
        std::env::remove_var("BLUEY_DEEPGRAM_LANGUAGE");
        std::env::remove_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS");
    }

    #[test]
    fn deepgram_url_merges_custom_keyterms() {
        let _guard = DEEPGRAM_URL_ENV_LOCK.lock().unwrap();
        std::env::set_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS", "0");
        std::env::set_var(
            "BLUEY_DEEPGRAM_KEYTERMS",
            "Asvad;LRU\nSecret Passage Ranch,REST API",
        );
        let session = ClaimedSttSession {
            token: "token".into(),
            account_id: "acct".into(),
            bluey_session_id: "sess".into(),
            provider: "deepgram".into(),
            model: "nova-3".into(),
            source: "microphone".into(),
            max_seconds: 60,
            expires_at_ms: now_ms() + 60_000,
            reserved_cents: 0,
            reserved_trial_seconds: 0,
        };
        let url = deepgram_realtime_url(&session);
        assert!(url.contains("keyterm=Asvad"));
        assert!(url.contains("keyterm=LRU"));
        assert!(url.contains("keyterm=Secret%20Passage%20Ranch"));
        assert!(url.contains("keyterm=REST%20API"));
        std::env::remove_var("BLUEY_DEEPGRAM_DEFAULT_KEYTERMS");
        std::env::remove_var("BLUEY_DEEPGRAM_KEYTERMS");
    }

    #[test]
    fn deepgram_frame_inspection_is_privacy_safe() {
        let payload = r#"{
          "type": "Results",
          "is_final": false,
          "speech_final": false,
          "channel": {
            "alternatives": [
              {"transcript": "hello realtime captions"}
            ]
          }
        }"#;
        let inspected = inspect_deepgram_relay_frame(payload);
        assert!(inspected.is_transcript_frame);
        assert!(!inspected.is_final);
        assert_eq!(inspected.transcript_chars, 23);
        assert_eq!(inspected.transcript_words, 3);
    }

    #[test]
    fn pcm16_i16le_stats_are_privacy_safe_levels() {
        let silence = vec![0_u8; 640];
        let silent = pcm16_i16le_stats(&silence);
        assert_eq!(silent.samples, 320);
        assert_eq!(silent.rms_dbfs, PCM16_DBFS_FLOOR);
        assert_eq!(silent.peak_dbfs, PCM16_DBFS_FLOOR);
        assert!(!silent.is_audible_for_stt());

        let mut audible = Vec::new();
        for _ in 0..320 {
            audible.extend_from_slice(&4_000_i16.to_le_bytes());
        }
        let stats = pcm16_i16le_stats(&audible);
        assert!(stats.rms_dbfs > -25.0);
        assert!(stats.peak_dbfs > -25.0);
        assert_eq!(stats.nonzero_percent, 100.0);
        assert!(stats.is_audible_for_stt());
    }

    #[test]
    fn estimate_cost_uses_deepgram_pricing() {
        assert_eq!(estimate_deepgram_cost_cents("nova-3", 0).unwrap(), 0);
        assert_eq!(estimate_deepgram_cost_cents("nova-3", 60).unwrap(), 3);
        assert_eq!(estimate_deepgram_bluey_cost_cents("nova-3", 60).unwrap(), 2);
    }
}
