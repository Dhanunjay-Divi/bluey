//! HTTPS client for `bluey-server`. Handles auth-token attachment,
//! 401-retry-with-refresh, 402 → InsufficientBalance, 429 → RateLimited.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{header, Client, Method, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};

use cue_core::{
    new_request_id, sanitize_observability_id, trace_id_from_env, BLUEY_REQUEST_ID_HEADER,
    BLUEY_TRACE_ID_HEADER,
};

use crate::{
    error::{Error, Result},
    tokens::{TokenStore, Tokens},
    types::{
        AuthResponse, CloudSessionBundle, InsufficientBalanceBody, RagQueryRequest,
        RagQueryResponse, SessionListResponse, SttSessionRequest, SttSessionResponse,
        SyncBatchRequest, SyncBatchResponse,
    },
};

/// Configuration for the cloud client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub user_agent: String,
    pub timeout: Duration,
    pub trace_id: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("BLUEY_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.bluey.dev".into()),
            user_agent: format!("bluey-cloud-client/{}", env!("CARGO_PKG_VERSION")),
            timeout: Duration::from_secs(60),
            trace_id: None,
        }
    }
}

/// Cloud client. Holds an HTTP client, the configured base URL, and a
/// pluggable token store. Cheap to clone (Arc internally).
#[derive(Clone)]
pub struct CloudClient {
    pub config: ClientConfig,
    http: Client,
    tokens: Arc<dyn TokenStore>,
    cached: Arc<Mutex<Option<Tokens>>>,
}

impl CloudClient {
    pub fn new(config: ClientConfig, tokens: Arc<dyn TokenStore>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(&config.user_agent)
            .timeout(config.timeout)
            .build()?;
        let cached = Arc::new(Mutex::new(tokens.load()?));
        Ok(Self {
            config,
            http,
            tokens,
            cached,
        })
    }

    /// Convenience constructor: keyring-backed store, default config.
    pub fn with_default_keyring() -> Result<Self> {
        Self::new(
            ClientConfig::default(),
            Arc::new(crate::tokens::KeyringStore::new()),
        )
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    /// Save tokens both to the persistent store and the in-memory cache.
    pub fn save_tokens(&self, tokens: Tokens) -> Result<()> {
        self.tokens.save(&tokens)?;
        *self.cached.lock().unwrap() = Some(tokens);
        Ok(())
    }

    /// Load tokens from cache (no I/O on the hot path).
    pub fn current_tokens(&self) -> Option<Tokens> {
        self.cached.lock().unwrap().clone()
    }

    /// Return a copy of this client that attaches a fixed trace id to all
    /// outgoing HTTP calls.
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.config.trace_id = sanitize_observability_id(&trace_id.into());
        self
    }

    /// Forget tokens in store + cache. Used by `bluey logout`.
    pub fn logout(&self) -> Result<()> {
        self.tokens.clear()?;
        *self.cached.lock().unwrap() = None;
        Ok(())
    }

    /// POST + parse JSON. No auth header (used for /auth/* endpoints).
    pub async fn post_json<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let resp = self
            .request_builder(Method::POST, path)
            .json(body)
            .send()
            .await?;
        Self::parse_or_err(resp).await
    }

    /// Raw POST returning the unparsed Response (caller inspects status).
    pub async fn raw_post<Req: Serialize>(&self, path: &str, body: &Req) -> Result<Response> {
        Ok(self
            .request_builder(Method::POST, path)
            .json(body)
            .send()
            .await?)
    }

    /// Authenticated GET. Auto-refresh on 401.
    pub async fn auth_get<Resp: DeserializeOwned>(&self, path: &str) -> Result<Resp> {
        self.auth_request(Method::GET, path, None::<&()>).await
    }

    /// Authenticated POST. Auto-refresh on 401.
    pub async fn auth_post<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        self.auth_request(Method::POST, path, Some(body)).await
    }

    pub async fn sync_batch(&self, batch: &SyncBatchRequest) -> Result<SyncBatchResponse> {
        self.auth_post("/sync/batch", batch).await
    }

    pub async fn list_cloud_sessions(&self, limit: Option<i64>) -> Result<SessionListResponse> {
        let path = match limit {
            Some(limit) => format!("/sync/sessions?limit={}", limit.clamp(1, 200)),
            None => "/sync/sessions".to_string(),
        };
        self.auth_get(&path).await
    }

    pub async fn load_cloud_session(&self, session_id: &str) -> Result<CloudSessionBundle> {
        self.auth_get(&format!("/sync/sessions/{session_id}")).await
    }

    pub async fn query_rag(&self, request: &RagQueryRequest) -> Result<RagQueryResponse> {
        self.auth_post("/rag/query", request).await
    }

    pub async fn create_stt_session(
        &self,
        request: &SttSessionRequest,
    ) -> Result<SttSessionResponse> {
        self.auth_post("/stt/session", request).await
    }

    /// Authenticated POST returning the raw response body stream.
    ///
    /// Used by `/router/complete/stream`; keeps the same 401 refresh and typed
    /// billing/error mapping as `auth_post`, but lets callers consume SSE bytes
    /// themselves on success.
    pub async fn auth_post_stream<Req: Serialize>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Response> {
        let resp = self.send_with_auth(Method::POST, path, Some(body)).await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            if !self.refresh_tokens().await? {
                return Err(Error::Unauthorized);
            }
            let resp = self.send_with_auth(Method::POST, path, Some(body)).await?;
            return Self::stream_or_err(resp).await;
        }
        Self::stream_or_err(resp).await
    }

    async fn auth_request<Req, Resp>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
    ) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        // First attempt with the current access token.
        let resp = self.send_with_auth(method.clone(), path, body).await?;
        if resp.status() != StatusCode::UNAUTHORIZED {
            return Self::parse_or_err(resp).await;
        }

        // 401 — try refreshing once.
        if !self.refresh_tokens().await? {
            return Err(Error::Unauthorized);
        }
        let resp = self.send_with_auth(method, path, body).await?;
        Self::parse_or_err(resp).await
    }

    async fn send_with_auth<Req: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Req>,
    ) -> Result<Response> {
        let access = self.current_tokens().ok_or(Error::Unauthorized)?.access;
        let mut req = self
            .request_builder(method, path)
            .header(header::AUTHORIZATION, format!("Bearer {access}"));
        if let Some(b) = body {
            req = req.json(b);
        }
        Ok(req.send().await?)
    }

    fn request_builder(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .http
            .request(method, self.url(path))
            .header(BLUEY_REQUEST_ID_HEADER, new_request_id());
        if let Some(trace_id) = self.current_trace_id() {
            req = req.header(BLUEY_TRACE_ID_HEADER, trace_id);
        }
        req
    }

    fn current_trace_id(&self) -> Option<String> {
        self.config
            .trace_id
            .as_deref()
            .and_then(sanitize_observability_id)
            .or_else(trace_id_from_env)
    }

    /// Try to refresh the token pair. Returns true on success.
    async fn refresh_tokens(&self) -> Result<bool> {
        let cur = match self.current_tokens() {
            Some(t) => t,
            None => return Ok(false),
        };
        if cur.refresh.is_empty() {
            return Ok(false);
        }
        let body = serde_json::json!({ "refresh_token": cur.refresh });
        let resp = self
            .request_builder(Method::POST, "/auth/refresh")
            .json(&body)
            .send()
            .await?;
        if resp.status() != StatusCode::OK {
            return Ok(false);
        }
        let auth: AuthResponse = resp.json().await?;
        self.save_tokens(Tokens {
            access: auth.access_token,
            refresh: auth.refresh_token,
            email: auth.account.email,
        })?;
        Ok(true)
    }

    /// Map server response to either parsed JSON or a typed error.
    async fn parse_or_err<Resp: DeserializeOwned>(resp: Response) -> Result<Resp> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json().await?);
        }
        match status {
            StatusCode::UNAUTHORIZED => Err(Error::Unauthorized),
            StatusCode::PAYMENT_REQUIRED => {
                let body: InsufficientBalanceBody =
                    resp.json().await.unwrap_or(InsufficientBalanceBody {
                        balance_cents: 0,
                        estimated_cost_cents: None,
                        needed_cents: None,
                        reason: None,
                        reload_url: None,
                    });
                if body.reason.as_deref() == Some("trial_ended") {
                    return Err(Error::TrialEnded);
                }
                let needed = body.needed_cents.or(body.estimated_cost_cents).unwrap_or(0);
                Err(Error::InsufficientBalance {
                    balance_cents: body.balance_cents,
                    needed_cents: needed,
                    reload_url: body.reload_url.unwrap_or_default(),
                })
            }
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after_secs = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                Err(Error::RateLimited { retry_after_secs })
            }
            other => {
                let body = resp.text().await.unwrap_or_default();
                tracing::warn!(
                    status = %other,
                    body = %log_safe_response_body(&body),
                    "cue-cloud-client server error"
                );
                Err(Error::Server {
                    status: other.as_u16(),
                })
            }
        }
    }

    async fn stream_or_err(resp: Response) -> Result<Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        match Self::parse_or_err::<serde_json::Value>(resp).await {
            Ok(_) => Err(Error::Server { status: 500 }),
            Err(e) => Err(e),
        }
    }

    /// Codex Stage 10: GET an unauthenticated public endpoint
    /// (e.g. /pricing/tiers, /admin/health). No Authorization header
    /// attached. Suitable for endpoints in the bluey-server public
    /// router gate.
    pub async fn public_get<Resp: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Resp> {
        let resp = self.request_builder(Method::GET, path).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(
                status = %status,
                body = %log_safe_response_body(&body),
                "public_get error"
            );
            return Err(Error::Server {
                status: status.as_u16(),
            });
        }
        Ok(resp.json().await?)
    }

    /// Codex Stage 16: bluey logout — forget tokens both in-cache and
    /// in the persistent store.
    pub fn clear_tokens(&self) -> Result<()> {
        self.tokens.clear()?;
        *self.cached.lock().unwrap() = None;
        Ok(())
    }

    /// Codex Stage 18 commit 2: public POST without bearer auth (used
    /// by /auth/link/exchange before tokens are available).
    pub async fn public_post<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let resp = self
            .request_builder(Method::POST, path)
            .json(body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(
                status = %status,
                body = %log_safe_response_body(&body),
                "public_post error"
            );
            return Err(Error::Server {
                status: status.as_u16(),
            });
        }
        Ok(resp.json().await?)
    }
}

pub(crate) fn log_safe_response_body(body: &str) -> String {
    const MAX_LOG_BYTES: usize = 256;
    if body.trim().is_empty() {
        return "<empty>".to_string();
    }

    let redacted = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(mut value) => {
            redact_json_value(&mut value);
            value.to_string()
        }
        Err(_) => body.to_string(),
    };

    truncate_log_value(&redacted, MAX_LOG_BYTES)
}

fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_sensitive_log_key(key) {
                    *value = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value);
            }
        }
        _ => {}
    }
}

fn is_sensitive_log_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("authorization")
        || key == "code"
        || key.ends_with("_code")
        || key == "url"
        || key.ends_with("_url")
}

fn truncate_log_value(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...<truncated>", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::MemoryStore;
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server_url: String) -> CloudClient {
        let config = ClientConfig {
            base_url: server_url,
            user_agent: "test".into(),
            timeout: Duration::from_secs(10),
            trace_id: None,
        };
        let store = Arc::new(MemoryStore::new());
        CloudClient::new(config, store).unwrap()
    }

    #[tokio::test]
    async fn parse_or_err_402_maps_to_insufficient_balance() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(ResponseTemplate::new(402).set_body_json(serde_json::json!({
                "balance_cents": 18,
                "estimated_cost_cents": 30,
                "reason": "insufficient_balance",
                "reload_url": "https://bluey.dev/reload"
            })))
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let result: Result<serde_json::Value> = client
            .auth_post(
                "/router/complete",
                &serde_json::json!({
                    "system": "x",
                    "user": "y",
                    "lane": "instant",
                }),
            )
            .await;
        match result {
            Err(Error::InsufficientBalance {
                balance_cents,
                needed_cents,
                ..
            }) => {
                assert_eq!(balance_cents, 18);
                assert_eq!(needed_cents, 30);
            }
            other => panic!("expected InsufficientBalance, got {other:?}"),
        }
    }

    #[test]
    fn log_safe_response_body_redacts_tokens_and_urls() {
        let body = serde_json::json!({
            "error": "bad",
            "access_token": "secret-access",
            "refresh_token": "secret-refresh",
            "verification_url": "https://bluey.dev/link?token=secret",
            "nested": { "device_code": "device-secret" }
        })
        .to_string();

        let safe = log_safe_response_body(&body);
        assert!(!safe.contains("secret-access"));
        assert!(!safe.contains("secret-refresh"));
        assert!(!safe.contains("device-secret"));
        assert!(!safe.contains("https://bluey.dev/link"));
        assert!(safe.contains("<redacted>"));
    }

    #[tokio::test]
    async fn parse_or_err_402_trial_ended_maps_to_trial_ended() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(ResponseTemplate::new(402).set_body_json(serde_json::json!({
                "balance_cents": 0,
                "reason": "trial_ended",
                "reload_url": "https://bluey.dev/reload"
            })))
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let r: Result<serde_json::Value> = client
            .auth_post(
                "/router/complete",
                &serde_json::json!({
                    "system": "x",
                    "user": "y",
                    "lane": "instant",
                }),
            )
            .await;
        assert!(matches!(r, Err(Error::TrialEnded)));
    }

    #[tokio::test]
    async fn parse_or_err_429_extracts_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "12"))
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let r: Result<serde_json::Value> = client
            .auth_post(
                "/router/complete",
                &serde_json::json!({
                    "system": "x",
                    "user": "y",
                    "lane": "instant",
                }),
            )
            .await;
        match r {
            Err(Error::RateLimited { retry_after_secs }) => assert_eq!(retry_after_secs, 12),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_post_stream_returns_raw_success_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string("data: hello\n\n"),
            )
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();

        let response = client
            .auth_post_stream(
                "/router/complete/stream",
                &serde_json::json!({ "ok": true }),
            )
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "data: hello\n\n");
    }

    #[tokio::test]
    async fn auth_post_stream_maps_402_before_streaming() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .respond_with(ResponseTemplate::new(402).set_body_json(serde_json::json!({
                "balance_cents": 1,
                "needed_cents": 9,
                "reason": "insufficient_balance",
                "reload_url": "https://bluey.dev/reload"
            })))
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();

        let result = client
            .auth_post_stream(
                "/router/complete/stream",
                &serde_json::json!({ "ok": true }),
            )
            .await;
        match result {
            Err(Error::InsufficientBalance {
                balance_cents,
                needed_cents,
                ..
            }) => {
                assert_eq!(balance_cents, 1);
                assert_eq!(needed_cents, 9);
            }
            other => panic!("expected InsufficientBalance, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_requests_send_trace_and_request_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header(BLUEY_TRACE_ID_HEADER, "trace-test"))
            .and(header_exists(BLUEY_REQUEST_ID_HEADER))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = client_for(server.uri()).with_trace_id("trace-test");
        client
            .save_tokens(Tokens {
                access: "a".into(),
                refresh: "r".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let _: serde_json::Value = client.auth_get("/account/me").await.unwrap();
    }

    #[tokio::test]
    async fn public_requests_send_request_header_without_trace_when_unset() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pricing/tiers"))
            .and(header_exists(BLUEY_REQUEST_ID_HEADER))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tiers": []
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        let _: serde_json::Value = client.public_get("/pricing/tiers").await.unwrap();
    }
}
