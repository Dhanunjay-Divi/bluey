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
    http::{HeaderMap, StatusCode},
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
use crate::api::router::provider_cost_guard;
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
const STT_ACCOUNT_LIVENESS_POLL_SECS: u64 = 2;
const PCM16_DBFS_FLOOR: f64 = -120.0;
const LIVE_STT_AUDIBLE_RMS_DBFS: f64 = -58.0;
const LIVE_STT_AUDIBLE_PEAK_DBFS: f64 = -34.0;
const DEFAULT_DEEPGRAM_ENDPOINTING_MS: u32 = 10;
const DEFAULT_DEEPGRAM_UTTERANCE_END_MS: Option<u32> = None;
const DEFAULT_DEEPGRAM_LANGUAGE: &str = "en-IN";
const DEFAULT_DEEPGRAM_NO_DELAY: bool = true;
const DEFAULT_DEEPGRAM_SMART_FORMAT: bool = false;
/// 16 kHz, mono, signed 16-bit linear PCM.
const DEEPGRAM_LINEAR16_BYTES_PER_SECOND: u64 = 32_000;
const MAX_DEEPGRAM_KEYTERMS: usize = 64;
const BLUEY_STT_SESSION_HEADER: &str = "x-bluey-stt-session";
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PcmFrameAdmissionError {
    OddLength,
    SessionAudioLimit,
    ByteRateLimit,
}

/// Permit Bluey's eight-read startup preface (8 * 4096 bytes = 1.024 seconds)
/// plus a small scheduling margin while keeping later PCM close to real time.
/// The allowance grows only from process-local monotonic elapsed time, so
/// client clocks and database clocks cannot create extra exposure.
const RELAY_PCM_JITTER_BURST_MS: u64 = 1_100;
const RELAY_PCM_JITTER_BURST_BYTES: u64 =
    DEEPGRAM_LINEAR16_BYTES_PER_SECOND * RELAY_PCM_JITTER_BURST_MS / 1_000;

fn max_relay_pcm_bytes(max_seconds: i64) -> u64 {
    u64::try_from(max_seconds.max(0))
        .unwrap_or(u64::MAX)
        .saturating_mul(DEEPGRAM_LINEAR16_BYTES_PER_SECOND)
}

fn admit_relay_pcm_frame(
    forwarded_audio_bytes: u64,
    frame_bytes: usize,
    max_seconds: i64,
    elapsed: Duration,
) -> Result<u64, PcmFrameAdmissionError> {
    if !frame_bytes.is_multiple_of(2) {
        return Err(PcmFrameAdmissionError::OddLength);
    }
    let frame_bytes =
        u64::try_from(frame_bytes).map_err(|_| PcmFrameAdmissionError::SessionAudioLimit)?;
    let updated = forwarded_audio_bytes
        .checked_add(frame_bytes)
        .ok_or(PcmFrameAdmissionError::SessionAudioLimit)?;
    if updated > max_relay_pcm_bytes(max_seconds) {
        return Err(PcmFrameAdmissionError::SessionAudioLimit);
    }
    let elapsed_bytes = elapsed
        .as_nanos()
        .saturating_mul(u128::from(DEEPGRAM_LINEAR16_BYTES_PER_SECOND))
        / 1_000_000_000_u128;
    let paced_limit = u64::try_from(elapsed_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(RELAY_PCM_JITTER_BURST_BYTES)
        .min(max_relay_pcm_bytes(max_seconds));
    if updated > paced_limit {
        return Err(PcmFrameAdmissionError::ByteRateLimit);
    }
    Ok(updated)
}

fn verified_relay_provider_seconds(audio_bytes: u64, max_seconds: i64) -> Option<i64> {
    if !audio_bytes.is_multiple_of(2) || audio_bytes > max_relay_pcm_bytes(max_seconds) {
        return None;
    }
    let seconds = audio_bytes.checked_add(DEEPGRAM_LINEAR16_BYTES_PER_SECOND - 1)?
        / DEEPGRAM_LINEAR16_BYTES_PER_SECOND;
    i64::try_from(seconds).ok()
}

fn relay_provider_usage(
    audio_bytes: u64,
    max_seconds: i64,
    audio_bytes_exact: bool,
) -> (i64, pricing::UsageProvenance) {
    if audio_bytes_exact {
        if let Some(seconds) = verified_relay_provider_seconds(audio_bytes, max_seconds) {
            return (seconds, pricing::UsageProvenance::Exact);
        }
    }
    (max_seconds.max(0), pricing::UsageProvenance::Missing)
}

fn relay_customer_billable_elapsed(
    wall_elapsed: Duration,
    forwarded_audio_bytes: u64,
    forwarded_audio_bytes_exact: bool,
    forwarded_audible_audio_chunks: u64,
    max_seconds: i64,
) -> Duration {
    if forwarded_audible_audio_chunks == 0 {
        return Duration::ZERO;
    }
    let session_cap = Duration::from_secs(u64::try_from(max_seconds.max(0)).unwrap_or(u64::MAX));
    let wall_elapsed = wall_elapsed.min(session_cap);
    if !forwarded_audio_bytes_exact {
        // Provider exposure is conservatively Missing/full-projection after an
        // ambiguous send, but customer/trial settlement must use only elapsed
        // time that Bluey can prove instead of charging that full projection.
        return wall_elapsed;
    }
    let pcm_elapsed = verified_relay_provider_seconds(forwarded_audio_bytes, max_seconds)
        .and_then(|seconds| u64::try_from(seconds).ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO);
    wall_elapsed.max(pcm_elapsed).min(session_cap)
}

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
    #[serde(default)]
    pub session_token: Option<String>,
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
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, (StatusCode, String)> {
    let session_token = stt_relay_session_token(&headers, query.session_token.as_deref())?;
    let session = claim_relay_session(&state, &account.id, session_token)?;
    let deepgram_key = state
        .config
        .upstream
        .deepgram_key(&session.token)
        .map(str::to_string);
    let Some(deepgram_key) = deepgram_key else {
        let _ = stt_accounting::settle_session(
            &state.pool,
            &session.token,
            &session.account_id,
            &session.model,
            0,
            "provider_not_configured",
            now_ms(),
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "STT relay is not configured".to_string(),
        ));
    };
    Ok(ws
        .on_upgrade(move |socket| async move {
            if let Err(err) = run_deepgram_relay(socket, state, session, deepgram_key).await {
                tracing::warn!(error = %err, "STT relay closed with error");
            }
        })
        .into_response())
}

fn stt_relay_session_token<'a>(
    headers: &'a HeaderMap,
    query_token: Option<&'a str>,
) -> Result<&'a str, (StatusCode, String)> {
    let header_token = headers
        .get(BLUEY_STT_SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let query_token = query_token.map(str::trim).filter(|value| !value.is_empty());

    if let (Some(header_token), Some(query_token)) = (header_token, query_token) {
        if header_token != query_token {
            return Err((
                StatusCode::BAD_REQUEST,
                "conflicting STT session credentials".to_string(),
            ));
        }
    }

    header_token.or(query_token).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "STT session credential is required".to_string(),
        )
    })
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

fn close_stt_session_without_dispatch(state: &AppState, session: &ClaimedSttSession, reason: &str) {
    if let Err(error) = stt_accounting::settle_session(
        &state.pool,
        &session.token,
        &session.account_id,
        &session.model,
        0,
        reason,
        now_ms(),
    ) {
        tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
            reason,
            error = %error,
            "failed to refund an unstarted STT relay session"
        );
    }
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
    let projected_bluey_cost =
        match estimate_deepgram_bluey_cost_cents(&session.model, session.max_seconds) {
            Ok(cost) => cost,
            Err((_, message)) => {
                close_stt_session_without_dispatch(&state, &session, "pricing_unavailable");
                anyhow::bail!(message)
            }
        };
    let url = deepgram_realtime_url(&session);
    let mut request = match url.into_client_request() {
        Ok(request) => request,
        Err(error) => {
            close_stt_session_without_dispatch(&state, &session, "provider_request_invalid");
            return Err(error.into());
        }
    };
    let authorization = match HeaderValue::from_str(&format!("Token {deepgram_key}")) {
        Ok(value) => value,
        Err(error) => {
            close_stt_session_without_dispatch(&state, &session, "provider_key_invalid");
            return Err(error.into());
        }
    };
    request.headers_mut().insert("Authorization", authorization);

    // The durable hold is the last local step before network dispatch. Local
    // URL/header validation failures above therefore create no paid exposure.
    let attempt_request_id = format!("stt-live-{}:attempt", session.token);
    let admission = match provider_cost_guard::reserve(
        &state.pool,
        state.config.upstream_spend_guard,
        &session.account_id,
        &format!("stt-live:{}", session.token),
        &attempt_request_id,
        &session.provider,
        &session.model,
        projected_bluey_cost,
        "stt_live_attempt",
        "transcription",
    ) {
        Ok(admission) => admission,
        Err(error) => {
            close_stt_session_without_dispatch(&state, &session, "spend_guard_unavailable");
            return Err(error);
        }
    };
    if let Some((close_reason, error)) = live_stt_admission_denial(&admission) {
        close_stt_session_without_dispatch(&state, &session, close_reason);
        anyhow::bail!(error)
    }
    let mut provider_guard = match admission {
        provider_cost_guard::Admission::Held(guard) => guard,
        provider_cost_guard::Admission::Unconfigured
        | provider_cost_guard::Admission::GlobalLimit => {
            unreachable!("live STT admission denial returned above")
        }
    };

    let (upstream, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(upstream) => upstream,
        Err(error) => {
            // A network attempt may have reached the provider. Terminalize the
            // conservative hold before refunding/releasing customer state.
            provider_guard.settle_conservative()?;
            close_stt_session_without_dispatch(&state, &session, "provider_connect_failed");
            return Err(error.into());
        }
    };
    let (mut client_tx, mut client_rx) = socket.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    let deadline = Duration::from_secs(session.max_seconds.max(1) as u64);
    let mut close_reason = "completed".to_string();
    let mut forwarded_audio_bytes = 0_u64;
    let mut forwarded_audio_bytes_exact = true;
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
                match message? {
                    ClientMessage::Binary(bytes) => {
                        if !bytes.is_empty() {
                            let updated_audio_bytes = match admit_relay_pcm_frame(
                                forwarded_audio_bytes,
                                bytes.len(),
                                session.max_seconds,
                                started.elapsed(),
                            ) {
                                Ok(updated) => updated,
                                Err(PcmFrameAdmissionError::OddLength) => {
                                    close_reason = "invalid_pcm_frame".to_string();
                                    let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
                                    break;
                                }
                                Err(PcmFrameAdmissionError::SessionAudioLimit) => {
                                    close_reason = "audio_byte_limit".to_string();
                                    let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
                                    break;
                                }
                                Err(PcmFrameAdmissionError::ByteRateLimit) => {
                                    close_reason = "audio_rate_limit".to_string();
                                    let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
                                    break;
                                }
                            };
                            // A select cancellation or transport error during
                            // this await makes delivery of the current frame
                            // ambiguous. Keep the full projection unless the
                            // upstream sink confirms the complete write.
                            forwarded_audio_bytes_exact = false;
                            upstream_tx
                                .send(UpstreamMessage::Binary(bytes.clone()))
                                .await?;
                            forwarded_audio_bytes = updated_audio_bytes;
                            forwarded_audio_bytes_exact = true;
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
                        } else {
                            upstream_tx.send(UpstreamMessage::Binary(bytes)).await?;
                        }
                    }
                    ClientMessage::Text(_) => {
                        close_reason = "client_control_rejected".to_string();
                        let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
                        break;
                    }
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
                close_reason = "client_to_provider_error".to_string();
                tracing::warn!(error = %err, "STT relay client-to-provider pipe failed");
            }
        }
        result = async {
            while let Some(message) = upstream_rx.next().await {
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
                close_reason = "provider_to_client_error".to_string();
                tracing::warn!(error = %err, "STT relay provider-to-client pipe failed");
            }
        }
        result = monitor_stt_account_liveness(&state, &session.account_id) => {
            match result {
                Err(SttAccountLivenessError::Deleted) => {
                    close_reason = "account_deleted".to_string();
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
                        "STT relay account was deleted"
                    );
                }
                Err(SttAccountLivenessError::Inactive) => {
                    close_reason = "account_inactive".to_string();
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
                        "STT relay account is restricted or expired"
                    );
                }
                Err(SttAccountLivenessError::CheckFailed(error)) => {
                    close_reason = "account_liveness_check_error".to_string();
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
                        error = %error,
                        "STT relay account liveness check failed"
                    );
                }
                Ok(()) => unreachable!("STT account liveness monitor only exits on failure"),
            }
            let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
            let _ = client_tx.send(ClientMessage::Close(None)).await;
        }
    }

    if close_reason == "account_deleted" {
        // Customer-owned session/usage rows are removed by privacy deletion,
        // but the opaque provider hold deliberately survives for the bounded
        // global spend window. Terminalize it before returning.
        settle_relay_provider_attempt(
            &session,
            &mut provider_guard,
            &attempt_request_id,
            started.elapsed(),
            forwarded_audio_bytes,
            forwarded_audio_bytes_exact,
        )?;
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

    let provider_elapsed = started.elapsed();
    let billable_elapsed = relay_customer_billable_elapsed(
        provider_elapsed,
        forwarded_audio_bytes,
        forwarded_audio_bytes_exact,
        forwarded_audible_audio_chunks,
        session.max_seconds,
    );
    let settle_reason = if forwarded_audible_audio_chunks == 0 {
        format!("{close_reason}:no_audible_audio")
    } else {
        close_reason
    };
    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
        bluey_session_ref = %cue_core::short_observability_ref(Some(&session.bluey_session_id)),
        source = %session.source,
        provider = %session.provider,
        model = %session.model,
        forwarded_audio_bytes,
        forwarded_audio_bytes_exact,
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
        &state.pool,
        &session,
        &mut provider_guard,
        &attempt_request_id,
        provider_elapsed,
        billable_elapsed,
        &settle_reason,
        forwarded_audio_bytes,
        forwarded_audio_bytes_exact,
        forwarded_audio_chunks,
    )?;
    Ok(())
}

fn live_stt_admission_denial(
    admission: &provider_cost_guard::Admission,
) -> Option<(&'static str, &'static str)> {
    match admission {
        provider_cost_guard::Admission::Held(_) => None,
        provider_cost_guard::Admission::Unconfigured => Some((
            "spend_guard_unconfigured",
            "paid STT route unexpectedly had zero projected exposure",
        )),
        provider_cost_guard::Admission::GlobalLimit => Some((
            "spend_guard_denied",
            "upstream spend guard denied STT relay dispatch",
        )),
    }
}

#[derive(Debug, thiserror::Error)]
enum SttAccountLivenessError {
    #[error("account_deleted")]
    Deleted,
    #[error("account_inactive")]
    Inactive,
    #[error("account liveness check failed: {0}")]
    CheckFailed(#[source] anyhow::Error),
}

fn ensure_stt_account_active(
    pool: &crate::db::DbPool,
    account_id: &str,
) -> Result<(), SttAccountLivenessError> {
    match Account::fetch_by_id(pool, account_id) {
        Ok(Some(account)) if !account.billing_restricted && !account.is_temporary_expired() => {
            Ok(())
        }
        Ok(Some(_)) => Err(SttAccountLivenessError::Inactive),
        Ok(None) => Err(SttAccountLivenessError::Deleted),
        Err(e) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                error = %e,
                "failed to verify STT account liveness"
            );
            Err(SttAccountLivenessError::CheckFailed(e))
        }
    }
}

async fn monitor_stt_account_liveness(
    state: &AppState,
    account_id: &str,
) -> Result<(), SttAccountLivenessError> {
    loop {
        tokio::time::sleep(Duration::from_secs(STT_ACCOUNT_LIVENESS_POLL_SECS)).await;
        ensure_stt_account_active(&state.pool, account_id)?;
    }
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
    let (sample_pairs, remainder) = raw[..sample_bytes].as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    for chunk in sample_pairs {
        let sample = i16::from_le_bytes(*chunk) as i32;
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

#[allow(clippy::too_many_arguments)]
fn finalize_relay_session(
    pool: &crate::db::DbPool,
    session: &ClaimedSttSession,
    provider_guard: &mut provider_cost_guard::ProviderCostGuard,
    attempt_request_id: &str,
    provider_elapsed: Duration,
    customer_elapsed: Duration,
    reason: &str,
    audio_bytes: u64,
    audio_bytes_exact: bool,
    audio_chunks: u64,
) -> anyhow::Result<()> {
    settle_relay_provider_attempt(
        session,
        provider_guard,
        attempt_request_id,
        provider_elapsed,
        audio_bytes,
        audio_bytes_exact,
    )?;
    let elapsed_ms = customer_elapsed.as_millis().min(i64::MAX as u128) as i64;
    let settled = stt_accounting::settle_session(
        pool,
        &session.token,
        &session.account_id,
        &session.model,
        elapsed_ms,
        reason,
        now_ms(),
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

fn settle_relay_provider_attempt(
    session: &ClaimedSttSession,
    provider_guard: &mut provider_cost_guard::ProviderCostGuard,
    attempt_request_id: &str,
    elapsed: Duration,
    audio_bytes: u64,
    audio_bytes_exact: bool,
) -> anyhow::Result<()> {
    let elapsed_ms = elapsed.as_millis().min(i64::MAX as u128) as i64;
    let (provider_seconds, usage_provenance) =
        relay_provider_usage(audio_bytes, session.max_seconds, audio_bytes_exact);
    let provider_pricing = pricing::lookup(&session.provider, &session.model)
        .ok_or_else(|| anyhow::anyhow!("missing live STT provider pricing"))?;
    let (provider_bluey_cents, _) = pricing::compute_cost(provider_pricing, provider_seconds, 0);
    provider_guard.settle(
        usage::UsageEvent {
            request_id: attempt_request_id.to_string(),
            kind: "stt_live_attempt".to_string(),
            task_type: Some("transcription".to_string()),
            lane: Some(session.source.clone()),
            provider: Some(session.provider.clone()),
            model: Some(session.model.clone()),
            input_tokens: provider_seconds,
            output_tokens: 0,
            latency_ms: elapsed_ms,
            cost_cents_to_bluey: provider_bluey_cents,
            cost_cents_to_customer: 0,
            was_speculative: false,
            was_fallback: false,
        },
        provider_bluey_cents,
        usage_provenance,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};
    use std::sync::Mutex;

    #[test]
    fn relay_session_token_prefers_header_without_query_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(
            BLUEY_STT_SESSION_HEADER,
            HeaderValue::from_static("header-token"),
        );

        assert_eq!(
            stt_relay_session_token(&headers, None).expect("header token"),
            "header-token"
        );
    }

    #[test]
    fn relay_session_token_keeps_legacy_query_compatibility() {
        assert_eq!(
            stt_relay_session_token(&HeaderMap::new(), Some("legacy-token"))
                .expect("legacy query token"),
            "legacy-token"
        );
    }

    #[test]
    fn relay_session_token_rejects_conflicting_credentials() {
        let mut headers = HeaderMap::new();
        headers.insert(
            BLUEY_STT_SESSION_HEADER,
            HeaderValue::from_static("header-token"),
        );

        let (status, message) =
            stt_relay_session_token(&headers, Some("query-token")).expect_err("conflict");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(message, "conflicting STT session credentials");
    }

    #[test]
    fn live_pcm_budget_rejects_audio_beyond_twenty_minutes_even_in_a_burst() {
        let max_bytes = max_relay_pcm_bytes(MAX_SESSION_SECONDS);
        assert_eq!(
            max_bytes,
            u64::try_from(MAX_SESSION_SECONDS).unwrap() * DEEPGRAM_LINEAR16_BYTES_PER_SECOND
        );
        assert_eq!(
            admit_relay_pcm_frame(
                max_bytes - 2,
                2,
                MAX_SESSION_SECONDS,
                Duration::from_secs(MAX_SESSION_SECONDS as u64),
            ),
            Ok(max_bytes)
        );
        assert_eq!(
            admit_relay_pcm_frame(
                max_bytes,
                2,
                MAX_SESSION_SECONDS,
                Duration::from_secs(MAX_SESSION_SECONDS as u64),
            ),
            Err(PcmFrameAdmissionError::SessionAudioLimit)
        );
    }

    #[test]
    fn live_pcm_frames_require_complete_i16_samples() {
        assert_eq!(
            admit_relay_pcm_frame(0, 1, 60, Duration::ZERO),
            Err(PcmFrameAdmissionError::OddLength)
        );
        assert_eq!(admit_relay_pcm_frame(0, 2, 60, Duration::ZERO), Ok(2));
        assert_eq!(verified_relay_provider_seconds(1, 60), None);
    }

    #[test]
    fn hostile_pcm_burst_cannot_front_load_a_twenty_minute_session() {
        let full_session = usize::try_from(max_relay_pcm_bytes(MAX_SESSION_SECONDS)).unwrap();
        assert_eq!(
            admit_relay_pcm_frame(0, full_session, MAX_SESSION_SECONDS, Duration::from_secs(1),),
            Err(PcmFrameAdmissionError::ByteRateLimit)
        );
    }

    #[test]
    fn bluey_eight_read_preface_and_normal_jitter_fit_but_ninth_frontload_does_not() {
        assert_eq!(RELAY_PCM_JITTER_BURST_BYTES, 35_200);
        let mut forwarded = 0;
        for _ in 0..8 {
            forwarded = admit_relay_pcm_frame(forwarded, 4_096, 60, Duration::ZERO).unwrap();
        }
        assert_eq!(forwarded, 32_768);
        assert_eq!(
            admit_relay_pcm_frame(forwarded, 4_096, 60, Duration::ZERO),
            Err(PcmFrameAdmissionError::ByteRateLimit)
        );
        assert_eq!(
            admit_relay_pcm_frame(forwarded, 5_632, 60, Duration::from_millis(100)),
            Ok(38_400),
            "100ms of monotonic elapsed time adds one normal PCM interval"
        );
    }

    #[test]
    fn customer_elapsed_uses_exact_pcm_for_audible_audio_without_silent_or_ambiguous_overcharge() {
        let ten_seconds = 10 * DEEPGRAM_LINEAR16_BYTES_PER_SECOND;
        assert_eq!(
            relay_customer_billable_elapsed(Duration::from_secs(1), ten_seconds, true, 0, 60,),
            Duration::ZERO,
            "fully silent audio remains free"
        );
        assert_eq!(
            relay_customer_billable_elapsed(Duration::from_secs(1), ten_seconds, true, 1, 60,),
            Duration::from_secs(10),
            "one confirmed audible chunk charges at least exact forwarded PCM duration"
        );
        assert_eq!(
            relay_customer_billable_elapsed(
                Duration::from_secs(2),
                ten_seconds,
                false,
                1,
                60,
            ),
            Duration::from_secs(2),
            "ambiguous provider sends retain conservative provider exposure without customer overcharge"
        );
    }

    #[test]
    fn customer_pcm_duration_never_exceeds_the_session_cap() {
        let max_bytes = max_relay_pcm_bytes(60);
        assert_eq!(
            relay_customer_billable_elapsed(Duration::from_secs(120), max_bytes, true, 1, 60,),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn live_provider_duration_is_exactly_derived_from_forwarded_pcm_bytes() {
        assert_eq!(verified_relay_provider_seconds(0, 60), Some(0));
        assert_eq!(verified_relay_provider_seconds(2, 60), Some(1));
        assert_eq!(
            verified_relay_provider_seconds(DEEPGRAM_LINEAR16_BYTES_PER_SECOND, 60),
            Some(1)
        );
        assert_eq!(
            verified_relay_provider_seconds(DEEPGRAM_LINEAR16_BYTES_PER_SECOND + 2, 60),
            Some(2)
        );
        assert_eq!(
            verified_relay_provider_seconds(max_relay_pcm_bytes(60) + 2, 60),
            None
        );
        assert_eq!(
            relay_provider_usage(DEEPGRAM_LINEAR16_BYTES_PER_SECOND, 60, true),
            (1, pricing::UsageProvenance::Exact)
        );
        assert_eq!(
            relay_provider_usage(DEEPGRAM_LINEAR16_BYTES_PER_SECOND, 60, false),
            (60, pricing::UsageProvenance::Missing)
        );
    }

    #[test]
    fn live_spend_guard_denial_keeps_an_explicit_closed_session_reason() {
        assert_eq!(
            live_stt_admission_denial(&provider_cost_guard::Admission::GlobalLimit),
            Some((
                "spend_guard_denied",
                "upstream spend guard denied STT relay dispatch"
            ))
        );
        assert_eq!(
            live_stt_admission_denial(&provider_cost_guard::Admission::Unconfigured),
            Some((
                "spend_guard_unconfigured",
                "paid STT route unexpectedly had zero projected exposure"
            ))
        );
    }

    static DEEPGRAM_URL_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_pool() -> crate::db::DbPool {
        let path = std::env::temp_dir().join(format!("bluey-stt-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).expect("open test pool");
        run_migrations(&pool).expect("migrate test pool");
        pool
    }

    fn reserve_live_provider_guard(
        pool: &crate::db::DbPool,
        session: &ClaimedSttSession,
        attempt_request_id: &str,
    ) -> provider_cost_guard::ProviderCostGuard {
        let projected_bluey_cost =
            estimate_deepgram_bluey_cost_cents(&session.model, session.max_seconds)
                .expect("priced Deepgram route");
        match provider_cost_guard::reserve(
            pool,
            Some(crate::config::UpstreamSpendGuard {
                limit_cents: 10_000,
                window_hours: 24,
            }),
            &session.account_id,
            &format!("stt-live:{}", session.token),
            attempt_request_id,
            &session.provider,
            &session.model,
            projected_bluey_cost,
            "stt_live_attempt",
            "transcription",
        )
        .expect("reserve provider exposure")
        {
            provider_cost_guard::Admission::Held(guard) => *guard,
            _ => panic!("expected live STT provider hold"),
        }
    }

    fn reserve_and_claim_live_session(
        pool: &crate::db::DbPool,
        account_id: &str,
        token: &str,
    ) -> ClaimedSttSession {
        stt_accounting::reserve_session(
            pool,
            ReserveSessionInput {
                account_id,
                bluey_session_id: "live-accounting",
                provider: "deepgram",
                model: "nova-3",
                source: "microphone",
                mode: "server_relay",
                token,
                max_seconds: 60,
                created_at_ms: 1_000,
                expires_at_ms: 61_000,
            },
        )
        .expect("reserve customer STT session");
        stt_accounting::claim_relay_session(pool, account_id, token, 2_000)
            .expect("claim customer STT session")
    }

    #[test]
    fn stt_account_liveness_distinguishes_active_closed_and_missing_accounts() {
        let pool = temp_pool();
        let account =
            Account::create(&pool, "stt-live@example.com", "hash").expect("create active account");
        assert!(ensure_stt_account_active(&pool, &account.id).is_ok());

        Account::restrict_billing(&pool, &account.id, "test", None).expect("restrict test account");
        assert!(matches!(
            ensure_stt_account_active(&pool, &account.id),
            Err(SttAccountLivenessError::Inactive)
        ));
        assert!(matches!(
            ensure_stt_account_active(&pool, "missing-account"),
            Err(SttAccountLivenessError::Deleted)
        ));
    }

    #[test]
    fn live_stt_provider_settlement_failure_blocks_customer_finalization() {
        let pool = temp_pool();
        let account =
            Account::create(&pool, "stt-a-before-b@example.com", "hash").expect("create account");
        let session = reserve_and_claim_live_session(&pool, &account.id, "stt-a-before-b");
        let attempt_request_id = "stt-live-stt-a-before-b:attempt";
        let mut provider_guard = reserve_live_provider_guard(&pool, &session, attempt_request_id);
        let before: (i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT balance_cents, reserved_cents, trial_seconds_remaining
                   FROM accounts WHERE id = ?1",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        crate::db::jobs_provider_cost_holds::fail_next_settlement_for_test();
        let error = finalize_relay_session(
            &pool,
            &session,
            &mut provider_guard,
            attempt_request_id,
            Duration::from_secs(10),
            Duration::from_secs(10),
            "completed",
            16_000,
            true,
            1,
        )
        .expect_err("provider settlement failure must stop before customer settlement");
        assert!(error
            .to_string()
            .contains("injected provider hold settlement failure"));
        drop(provider_guard); // crash-safety retry may settle A, but never B.

        let conn = pool.get().unwrap();
        let after: (i64, i64, i64) = conn
            .query_row(
                "SELECT balance_cents, reserved_cents, trial_seconds_remaining
                   FROM accounts WHERE id = ?1",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(after, before, "customer money/trial state must not advance");
        let (consumed_seconds, ended_at_ms): (i64, Option<i64>) = conn
            .query_row(
                "SELECT consumed_seconds, ended_at_ms
                   FROM stt_sessions WHERE session_token = ?1",
                rusqlite::params![session.token],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(consumed_seconds, 0);
        assert_eq!(ended_at_ms, None);
        let customer_roots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_events
                  WHERE account_id = ?1 AND kind = 'stt'",
                rusqlite::params![account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(customer_roots, 0);
    }

    #[test]
    fn live_stt_trial_and_mixed_sessions_split_upstream_and_customer_cost_once() {
        for (case, trial_seconds, expected_trial, expected_billable) in
            [("trial", 60, 10, 0), ("mixed", 5, 5, 5)]
        {
            let pool = temp_pool();
            let account =
                Account::create(&pool, &format!("stt-{case}-accounting@example.com"), "hash")
                    .expect("create account");
            pool.get()
                .unwrap()
                .execute(
                    "UPDATE accounts SET trial_seconds_remaining = ?1 WHERE id = ?2",
                    rusqlite::params![trial_seconds, account.id],
                )
                .unwrap();
            if case == "mixed" {
                crate::db::balance::credit_internal(&pool, &account.id, 100, "stt-mixed-test")
                    .unwrap();
            }
            let token = format!("stt-{case}-exact");
            let session = reserve_and_claim_live_session(&pool, &account.id, &token);
            let attempt_request_id = format!("stt-live-{token}:attempt");
            let mut provider_guard =
                reserve_live_provider_guard(&pool, &session, &attempt_request_id);

            finalize_relay_session(
                &pool,
                &session,
                &mut provider_guard,
                &attempt_request_id,
                Duration::from_secs(10),
                Duration::from_secs(10),
                "completed",
                160_000,
                true,
                10,
            )
            .expect("settle live STT A then B");

            let pricing = pricing::lookup("deepgram", "nova-3").unwrap();
            let expected_provider_seconds = 5;
            let (expected_bluey_cost, _) =
                pricing::compute_cost(pricing, expected_provider_seconds, 0);
            let (_, expected_customer_cost) = pricing::compute_cost(pricing, expected_billable, 0);
            let conn = pool.get().unwrap();
            let attempt: (i64, i64, i64) = conn
                .query_row(
                    "SELECT input_tokens, cost_cents_to_bluey, cost_cents_to_customer
                       FROM usage_events
                      WHERE account_id = ?1 AND kind = 'stt_live_attempt'",
                    rusqlite::params![account.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                attempt,
                (expected_provider_seconds, expected_bluey_cost, 0),
                "{case}"
            );
            let root: (i64, i64, i64) = conn
                .query_row(
                    "SELECT input_tokens, cost_cents_to_bluey, cost_cents_to_customer
                       FROM usage_events
                      WHERE account_id = ?1 AND kind = 'stt'",
                    rusqlite::params![account.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(root, (10, 0, expected_customer_cost), "{case}");
            let settled: (i64, i64) = conn
                .query_row(
                    "SELECT settled_trial_seconds, consumed_seconds
                       FROM stt_sessions WHERE session_token = ?1",
                    rusqlite::params![token],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(settled, (expected_trial, 10), "{case}");
            drop(conn);

            let summary = crate::db::usage::provider_routing_summary(&pool, 24).unwrap();
            assert_eq!(summary.total_events, 1, "{case}");
            assert_eq!(summary.cost_cents_to_bluey, expected_bluey_cost, "{case}");
            assert_eq!(
                summary.cost_cents_to_customer, expected_customer_cost,
                "{case}"
            );
        }
    }

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
