//! HTTPS client for `bluey-server`. Handles auth-token attachment,
//! 401-retry-with-refresh, 402 → InsufficientBalance, 429 → RateLimited.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::{header, Client, Method, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    error::{Error, Result},
    tokens::{TokenStore, Tokens},
    types::{AuthResponse, InsufficientBalanceBody},
};

/// Configuration for the cloud client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub user_agent: String,
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("BLUEY_API_BASE_URL")
                .unwrap_or_else(|_| "https://api.bluey.dev".into()),
            user_agent: format!("bluey-cloud-client/{}", env!("CARGO_PKG_VERSION")),
            timeout: Duration::from_secs(60),
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
        Ok(Self { config, http, tokens, cached })
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
        let resp = self.http.post(self.url(path)).json(body).send().await?;
        Self::parse_or_err(resp).await
    }

    /// Raw POST returning the unparsed Response (caller inspects status).
    pub async fn raw_post<Req: Serialize>(&self, path: &str, body: &Req) -> Result<Response> {
        Ok(self.http.post(self.url(path)).json(body).send().await?)
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
        let access = self
            .current_tokens()
            .ok_or(Error::Unauthorized)?
            .access;
        let mut req = self
            .http
            .request(method, self.url(path))
            .header(header::AUTHORIZATION, format!("Bearer {access}"));
        if let Some(b) = body {
            req = req.json(b);
        }
        Ok(req.send().await?)
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
        let resp = self.http.post(self.url("/auth/refresh")).json(&body).send().await?;
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
                let body: InsufficientBalanceBody = resp.json().await.unwrap_or(InsufficientBalanceBody {
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
                Err(Error::Server { status: other.as_u16(), body })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::MemoryStore;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for(server_url: String) -> CloudClient {
        let config = ClientConfig {
            base_url: server_url,
            user_agent: "test".into(),
            timeout: Duration::from_secs(10),
        };
        let store = Arc::new(MemoryStore::new());
        CloudClient::new(config, store).unwrap()
    }

    #[tokio::test]
    async fn parse_or_err_402_maps_to_insufficient_balance() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(
                ResponseTemplate::new(402).set_body_json(serde_json::json!({
                    "balance_cents": 18,
                    "estimated_cost_cents": 30,
                    "reason": "insufficient_balance",
                    "reload_url": "https://bluey.dev/reload"
                })),
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
            Err(Error::InsufficientBalance { balance_cents, needed_cents, .. }) => {
                assert_eq!(balance_cents, 18);
                assert_eq!(needed_cents, 30);
            }
            other => panic!("expected InsufficientBalance, got {other:?}"),
        }
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
}
