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
                            if forwarded_audio_chunks == 1 || forwarded_audio_chunks % 50 == 0 {
                                let stats = pcm16_i16le_stats(bytes.as_ref());
                                tracing::info!(
                                    account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
                                    source = %session.source,
                                    provider = %session.provider,
                                    model = %session.model,
                                    audio_chunks = forwarded_audio_chunks,
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
                    UpstreamMessage::Text(text) => client_tx.send(ClientMessage::Text(text)).await?,
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
            "STT relay stopped because account was deleted; skipping billing settlement"
        );
        return Ok(());
    }

    let billable_elapsed = if forwarded_audio_bytes == 0 {
        Duration::ZERO
    } else {
        started.elapsed()
    };
    let settle_reason = if forwarded_audio_bytes == 0 {
        format!("{close_reason}:no_audio")
    } else {
        close_reason
    };
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
    let mut url = format!(
        "{base}?model={}&encoding=linear16&sample_rate=16000&channels=1&punctuate=true&smart_format=true&interim_results=true&endpointing=300&utterance_end_ms=1000&vad_events=true",
        url_escape(&session.model)
    );
    if let Ok(language) = std::env::var("BLUEY_DEEPGRAM_LANGUAGE") {
        let language = language.trim();
        if !language.is_empty() {
            url.push_str("&language=");
            url.push_str(&url_escape(language));
        }
    }
    url
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

    #[test]
    fn random_token_is_url_safe_and_long() {
        let token = random_token();
        assert!(token.len() >= 40);
        assert!(!token.contains('+'));
        assert!(!token.contains('/'));
    }

    #[test]
    fn deepgram_url_escapes_model() {
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
        assert!(url.contains("endpointing=300"));
        assert!(url.contains("utterance_end_ms=1000"));
        assert!(url.contains("vad_events=true"));
        assert!(!url.contains("Token "));
    }

    #[test]
    fn pcm16_i16le_stats_are_privacy_safe_levels() {
        let silence = vec![0_u8; 640];
        let silent = pcm16_i16le_stats(&silence);
        assert_eq!(silent.samples, 320);
        assert_eq!(silent.rms_dbfs, PCM16_DBFS_FLOOR);
        assert_eq!(silent.peak_dbfs, PCM16_DBFS_FLOOR);

        let mut audible = Vec::new();
        for _ in 0..320 {
            audible.extend_from_slice(&4_000_i16.to_le_bytes());
        }
        let stats = pcm16_i16le_stats(&audible);
        assert!(stats.rms_dbfs > -25.0);
        assert!(stats.peak_dbfs > -25.0);
        assert_eq!(stats.nonzero_percent, 100.0);
    }

    #[test]
    fn estimate_cost_uses_deepgram_pricing() {
        assert_eq!(estimate_deepgram_cost_cents("nova-3", 0).unwrap(), 0);
        assert_eq!(estimate_deepgram_cost_cents("nova-3", 60).unwrap(), 3);
        assert_eq!(estimate_deepgram_bluey_cost_cents("nova-3", 60).unwrap(), 2);
    }
}
