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
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::HeaderValue, Message as UpstreamMessage,
};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::{balance, usage};
use crate::pricing;

const DEFAULT_MAX_SECONDS: i64 = 10 * 60;
const MAX_SESSION_SECONDS: i64 = 20 * 60;

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
pub struct SttRelayQuery {
    pub session_token: String,
}

#[derive(Debug, Clone)]
struct ClaimedSttSession {
    token: String,
    account_id: String,
    bluey_session_id: String,
    provider: String,
    model: String,
    source: String,
    max_seconds: i64,
    expires_at_ms: i64,
}

pub async fn create_session(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<SttSessionRequest>,
) -> Result<Json<SttSessionResponse>, (StatusCode, String)> {
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

    let balance = balance::current_balance(&state.pool, &account.id).map_err(internal)?;
    let billable_ceiling_seconds = (max_seconds - account.trial_seconds_remaining).max(0);
    let estimated_cost_cents = estimate_deepgram_cost_cents(&model, billable_ceiling_seconds)?;
    if billable_ceiling_seconds > 0 && balance < estimated_cost_cents {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "balance is required before starting this STT session".into(),
        ));
    }

    let token = random_token();
    let now = now_ms();
    let expires_at = now + (max_seconds * 1000);
    let mode = "server_relay";
    state
        .pool
        .get()
        .map_err(internal)?
        .execute(
            "INSERT INTO stt_sessions (
                session_token, account_id, bluey_session_id, provider, model, source,
                mode, max_seconds, created_at_ms, expires_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                token,
                account.id,
                req.session_id,
                provider,
                model,
                req.source,
                mode,
                max_seconds,
                now,
                expires_at,
            ],
        )
        .map_err(internal)?;

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

fn claim_relay_session(
    state: &AppState,
    account_id: &str,
    token: &str,
) -> Result<ClaimedSttSession, (StatusCode, String)> {
    if token.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "session_token is required".into()));
    }
    let now = now_ms();
    let conn = state.pool.get().map_err(internal)?;
    let session = conn
        .query_row(
            "SELECT account_id, bluey_session_id, provider, model, source, max_seconds, expires_at_ms
             FROM stt_sessions
             WHERE session_token = ?1 AND account_id = ?2",
            params![token, account_id],
            |row| {
                Ok(ClaimedSttSession {
                    token: token.to_string(),
                    account_id: row.get(0)?,
                    bluey_session_id: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    source: row.get(4)?,
                    max_seconds: row.get(5)?,
                    expires_at_ms: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "invalid STT session".to_string()))?;
    if session.expires_at_ms <= now {
        return Err((StatusCode::GONE, "STT session expired".to_string()));
    }
    if session.provider != "deepgram" {
        return Err((
            StatusCode::BAD_REQUEST,
            "only deepgram STT relay is enabled".to_string(),
        ));
    }
    let updated = conn
        .execute(
            "UPDATE stt_sessions
                SET started_at_ms = ?1
              WHERE session_token = ?2
                AND account_id = ?3
                AND started_at_ms IS NULL
                AND ended_at_ms IS NULL
                AND expires_at_ms > ?1",
            params![now, token, account_id],
        )
        .map_err(internal)?;
    if updated != 1 {
        return Err((
            StatusCode::CONFLICT,
            "STT session is already active or closed".to_string(),
        ));
    }
    Ok(session)
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

    tokio::select! {
        _ = tokio::time::sleep(deadline) => {
            close_reason = "max_seconds".to_string();
            let _ = client_tx.send(ClientMessage::Text("{\"type\":\"bluey.stt_session_limit\"}".into())).await;
            let _ = upstream_tx.send(UpstreamMessage::Close(None)).await;
        }
        result = async {
            while let Some(message) = client_rx.next().await {
                match message? {
                    ClientMessage::Binary(bytes) => upstream_tx.send(UpstreamMessage::Binary(bytes)).await?,
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
                close_reason = "client_to_provider_error".to_string();
                tracing::warn!(error = %err, "STT relay client-to-provider pipe failed");
            }
        }
        result = async {
            while let Some(message) = upstream_rx.next().await {
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
                close_reason = "provider_to_client_error".to_string();
                tracing::warn!(error = %err, "STT relay provider-to-client pipe failed");
            }
        }
    }

    finalize_relay_session(&state, &session, started.elapsed(), &close_reason)?;
    Ok(())
}

fn deepgram_realtime_url(session: &ClaimedSttSession) -> String {
    let base = std::env::var("BLUEY_TEST_DEEPGRAM_WS_URL")
        .unwrap_or_else(|_| "wss://api.deepgram.com/v1/listen".to_string());
    format!(
        "{base}?model={}&encoding=linear16&sample_rate=16000&channels=1&punctuate=true&smart_format=true&interim_results=true",
        url_escape(&session.model)
    )
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

fn finalize_relay_session(
    state: &AppState,
    session: &ClaimedSttSession,
    elapsed: Duration,
    reason: &str,
) -> anyhow::Result<()> {
    let elapsed_ms = elapsed.as_millis().clamp(1, i64::MAX as u128) as i64;
    let elapsed_seconds = ((elapsed_ms + 999) / 1000).max(1);
    let conn = state.pool.get()?;
    let trial_remaining: i64 = conn.query_row(
        "SELECT trial_seconds_remaining FROM accounts WHERE id = ?1",
        params![&session.account_id],
        |row| row.get(0),
    )?;
    drop(conn);

    let free_seconds = trial_remaining.max(0).min(elapsed_seconds);
    if free_seconds > 0 {
        let _ =
            balance::consume_trial_seconds(&state.pool, &session.account_id, free_seconds * 1000)?;
    }
    let billable_seconds = elapsed_seconds - free_seconds;
    let pricing = pricing::lookup("deepgram", &session.model)
        .ok_or_else(|| anyhow::anyhow!("missing Deepgram pricing for {}", session.model))?;
    let (bluey_cost, customer_cost) = pricing::compute_cost(pricing, billable_seconds, 0);
    if customer_cost > 0 && !balance::deduct(&state.pool, &session.account_id, customer_cost)? {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&session.account_id),
            customer_cost,
            billable_seconds,
            "STT relay overrun absorbed by Bluey because balance was exhausted at close"
        );
    }
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
            input_tokens: elapsed_seconds,
            output_tokens: 0,
            latency_ms: elapsed_ms,
            cost_cents_to_bluey: bluey_cost,
            cost_cents_to_customer: customer_cost,
            was_speculative: false,
            was_fallback: false,
        },
    )?;
    state.pool.get()?.execute(
        "UPDATE stt_sessions
            SET consumed_seconds = ?1,
                ended_at_ms = ?2,
                relay_close_reason = ?3
          WHERE session_token = ?4",
        params![elapsed_seconds, now_ms(), reason, &session.token],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::AppState, config::Config, db};
    use std::{path::PathBuf, sync::Arc};

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
        };
        let url = deepgram_realtime_url(&session);
        assert!(url.contains("model=nova%203%2Ftest"));
        assert!(url.contains("interim_results=true"));
        assert!(!url.contains("Token "));
    }

    #[test]
    fn claim_relay_session_is_single_use() {
        let state = temp_state();
        let account = crate::db::accounts::Account::create(
            &state.pool,
            "relay-single-use@example.com",
            "hash",
        )
        .unwrap();
        let token = "single-use-token";
        state
            .pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO stt_sessions (
                    session_token, account_id, bluey_session_id, provider, model, source,
                    mode, max_seconds, created_at_ms, expires_at_ms
                 ) VALUES (?1, ?2, 'sess', 'deepgram', 'nova-3', 'microphone', 'server_relay', 60, ?3, ?4)",
                params![token, account.id, now_ms(), now_ms() + 60_000],
            )
            .unwrap();
        assert!(claim_relay_session(&state, &account.id, token).is_ok());
        let second = claim_relay_session(&state, &account.id, token).unwrap_err();
        assert_eq!(second.0, StatusCode::CONFLICT);
    }

    #[test]
    fn claim_relay_session_requires_matching_account() {
        let state = temp_state();
        let account = crate::db::accounts::Account::create(
            &state.pool,
            "relay-account-owner@example.com",
            "hash",
        )
        .unwrap();
        let token = "account-bound-token";
        state
            .pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO stt_sessions (
                    session_token, account_id, bluey_session_id, provider, model, source,
                    mode, max_seconds, created_at_ms, expires_at_ms
                 ) VALUES (?1, ?2, 'sess', 'deepgram', 'nova-3', 'microphone', 'server_relay', 60, ?3, ?4)",
                params![token, account.id, now_ms(), now_ms() + 60_000],
            )
            .unwrap();
        let err = claim_relay_session(&state, "other-account", token).unwrap_err();
        assert_eq!(err.0, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn estimate_cost_uses_deepgram_pricing() {
        assert_eq!(estimate_deepgram_cost_cents("nova-3", 0).unwrap(), 0);
        assert!(estimate_deepgram_cost_cents("nova-3", 60).unwrap() > 0);
    }

    fn temp_state() -> AppState {
        let path = std::env::temp_dir().join(format!("bluey-stt-{}.db", uuid::Uuid::new_v4()));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        AppState {
            pool,
            config: Arc::new(Config {
                port: 8080,
                db_path: PathBuf::from(":memory:"),
                jwt_secret: "x".repeat(32),
                public_url: "http://localhost:8080".into(),
                stripe_secret_key: None,
                stripe_webhook_secret: None,
                upstream: crate::config::UpstreamKeys {
                    deepgram_api_key: Some("dg-test".into()),
                    ..Default::default()
                },
                smtp: None,
                admin_emails: vec![],
            }),
            rate_limiters: crate::rate_limit::RateLimiters::default(),
            provider_health: crate::provider_health::ProviderHealth::default(),
        }
    }
}
