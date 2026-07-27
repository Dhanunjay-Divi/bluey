//! OAuth flow driver — token exchange/refresh + the interactive connect flow.
//!
//! Async. Uses `reqwest` (rustls) for the token endpoint. The interactive
//! `connect_interactive` orchestrates PKCE → loopback bind → authorize URL →
//! browser open (injected) → code capture → exchange, and returns [`CalTokens`]
//! ready to persist. `valid_access_token` is the per-request accessor B/C call:
//! it loads, refreshes-if-expired, persists, and hands back a live token.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::authorize::build_authorize_url;
use crate::loopback::{accept_code, bind_loopback};
use crate::pkce::{random_state, Pkce};
use crate::provider::{Provider, ProviderConfig};
use crate::tokens::{is_expired, CalTokenStore, CalTokens, DEFAULT_EXPIRY_SKEW_SECS};

/// The subset of an OAuth token-endpoint response we consume.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    /// Providers may omit this on refresh (Google does) — keep the old one then.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Lifetime of `access_token` in seconds.
    #[serde(default)]
    pub expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(120);

/// Wall-clock now in epoch seconds.
fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// POST the token endpoint (form-encoded, no client secret) and parse the JSON
/// response. Shared by exchange + refresh.
async fn post_token_form(token_url: &str, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let client = reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .context("build OAuth HTTP client")?;
    let resp = client
        .post(token_url)
        .form(form)
        .send()
        .await
        .with_context(|| format!("POST token endpoint {token_url}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .context("read token endpoint response body")?;
    if !status.is_success() {
        let parsed = serde_json::from_str::<TokenErrorResponse>(&body).ok();
        let code = parsed
            .as_ref()
            .map(|error| error.error.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("token_exchange_failed");
        let description = parsed
            .as_ref()
            .map(|error| sanitize_oauth_message(&error.error_description))
            .filter(|value| !value.is_empty());
        return Err(match description {
            Some(description) => {
                anyhow!("calendar token exchange failed ({code}, HTTP {status}): {description}")
            }
            None => anyhow!("calendar token exchange failed ({code}, HTTP {status})"),
        });
    }
    let response =
        serde_json::from_str::<TokenResponse>(&body).context("parse calendar token response")?;
    if response.access_token.trim().is_empty() {
        return Err(anyhow!("calendar token response contained no access token"));
    }
    Ok(response)
}

fn sanitize_oauth_message(message: &str) -> String {
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(300)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Exchange an authorization `code` for tokens (`grant_type=authorization_code`).
pub async fn exchange_code(
    cfg: &ProviderConfig,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> Result<TokenResponse> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", cfg.client_id.as_str()),
        ("code_verifier", verifier),
    ];
    post_token_form(&cfg.token_url, &form).await
}

/// Refresh an access token (`grant_type=refresh_token`). Google may omit a new
/// refresh_token in the response — callers keep the prior one in that case.
pub async fn refresh(cfg: &ProviderConfig, refresh_token: &str) -> Result<TokenResponse> {
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", cfg.client_id.as_str()),
    ];
    post_token_form(&cfg.token_url, &form).await
}

/// Turn a [`TokenResponse`] into [`CalTokens`], carrying forward the prior
/// refresh token when the response omits one, and computing an absolute expiry.
fn to_cal_tokens(
    resp: TokenResponse,
    prior_refresh: Option<&str>,
    email: String,
    now: u64,
) -> CalTokens {
    let refresh = resp
        .refresh_token
        .filter(|r| !r.trim().is_empty())
        .or_else(|| prior_refresh.map(str::to_string))
        .unwrap_or_default();
    // Default to a conservative 1h lifetime if the provider omits expires_in.
    let expires_in = resp.expires_in.unwrap_or(3600);
    CalTokens {
        access: resp.access_token,
        refresh,
        expires_at_epoch: now.saturating_add(expires_in),
        email,
    }
}

/// Run the full interactive connect flow for `provider` and return fresh
/// [`CalTokens`] (NOT yet persisted — the caller stores them).
///
/// `open_browser` is injected so the daemon supplies its own
/// `open`/`xdg-open`/`start` shell-out (and tests a no-op). This keeps the crate
/// free of any browser dependency.
pub async fn connect_interactive(
    provider: Provider,
    open_browser: impl FnOnce(&str) -> Result<()>,
) -> Result<CalTokens> {
    let cfg = provider.config();
    cfg.validate()?;
    let pkce = Pkce::generate()?;
    let state = random_state()?;

    // Bind the loopback listener FIRST so we know the port for the redirect_uri.
    let (listener, port) = bind_loopback(&cfg.loopback_host).await?;
    let redirect_uri = format!("http://{}:{port}", cfg.loopback_host);

    let authorize_url = build_authorize_url(&cfg, &redirect_uri, &pkce.challenge, &state)?;
    open_browser(&authorize_url).context("open calendar authorization in the system browser")?;

    let code = tokio::time::timeout(AUTHORIZATION_TIMEOUT, accept_code(listener, &state))
        .await
        .map_err(|_| anyhow!("calendar authorization timed out; retry from Bluey"))??;
    let resp = exchange_code(&cfg, &redirect_uri, &code, &pkce.verifier).await?;

    // The email label is filled in by B/C (a userinfo/profile call) — the core
    // flow returns it empty and lets the provider client enrich it.
    let tokens = to_cal_tokens(resp, None, String::new(), now_epoch_secs());
    if tokens.refresh.trim().is_empty() {
        return Err(anyhow!(
            "calendar provider returned no refresh token; revoke Bluey's prior \
             calendar grant and connect again"
        ));
    }
    Ok(tokens)
}

/// Load tokens from `store`, refresh if expired, persist the refresh, and
/// return a live access token. This is the accessor B and C call before each
/// calendar API request.
pub async fn valid_access_token(
    store: &dyn CalTokenStore,
    cfg: &ProviderConfig,
    now_epoch: u64,
) -> Result<String> {
    cfg.validate()?;
    let tokens = store
        .load()?
        .ok_or_else(|| anyhow!("no calendar tokens stored; connect the provider first"))?;

    if !is_expired(&tokens, now_epoch, DEFAULT_EXPIRY_SKEW_SECS) {
        return Ok(tokens.access);
    }

    if tokens.refresh.trim().is_empty() {
        return Err(anyhow!(
            "access token expired and no refresh token available; reconnect required"
        ));
    }

    let resp = refresh(cfg, &tokens.refresh).await?;
    let refreshed = to_cal_tokens(resp, Some(&tokens.refresh), tokens.email, now_epoch);
    store.save(&refreshed)?;
    Ok(refreshed.access)
}

/// Serialize the load → refresh → persist transaction for every consumer of one
/// provider store. Sharing only [`CalTokenStore`] is insufficient: two callers
/// can both load the same near-expiry refresh token, rotate it concurrently, and
/// let the later save overwrite the provider's newest refresh token.
pub async fn valid_access_token_serialized(
    store: &dyn CalTokenStore,
    cfg: &ProviderConfig,
    now_epoch: u64,
    operation: &tokio::sync::Mutex<()>,
) -> Result<String> {
    let _guard = operation.lock().await;
    valid_access_token(store, cfg, now_epoch).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::MemoryCalStore;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Debug)]
    struct CapturedTokenRequest {
        head: String,
        body: String,
    }

    async fn spawn_token_endpoint(
        status: u16,
        response_body: &str,
    ) -> (String, tokio::task::JoinHandle<CapturedTokenRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock token endpoint");
        let address = listener.local_addr().expect("mock token endpoint address");
        let response_body = response_body.to_string();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept token request");
            let mut request = Vec::new();
            let mut chunk = [0u8; 1_024];
            let header_end = loop {
                let count = stream.read(&mut chunk).await.expect("read token request");
                assert!(count > 0, "token request ended before its headers");
                request.extend_from_slice(&chunk[..count]);
                if let Some(index) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break index + 4;
                }
            };

            let head = String::from_utf8(request[..header_end].to_vec())
                .expect("token request headers are UTF-8");
            let content_length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().expect("valid content length"))
                })
                .expect("token request content length");
            while request.len() < header_end + content_length {
                let count = stream
                    .read(&mut chunk)
                    .await
                    .expect("read token request body");
                assert!(count > 0, "token request body ended early");
                request.extend_from_slice(&chunk[..count]);
            }
            let body = String::from_utf8(request[header_end..header_end + content_length].to_vec())
                .expect("token request body is UTF-8");

            let reason = if status == 200 { "OK" } else { "Bad Request" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write token response");
            CapturedTokenRequest { head, body }
        });
        (format!("http://{address}/token"), task)
    }

    fn configured(provider: Provider) -> ProviderConfig {
        let mut config = provider.config();
        config.client_id = match provider {
            Provider::Google => "test-public-client.apps.googleusercontent.com",
            Provider::Microsoft => "00000000-0000-4000-8000-000000000000",
        }
        .to_string();
        config
    }

    #[test]
    fn token_response_deserializes_partial() {
        // A refresh response that omits refresh_token (Google's behavior).
        let json = r#"{"access_token":"a1","expires_in":3599,"token_type":"Bearer"}"#;
        let parsed: TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.access_token, "a1");
        assert!(parsed.refresh_token.is_none());
        assert_eq!(parsed.expires_in, Some(3599));
    }

    #[test]
    fn to_cal_tokens_keeps_prior_refresh_when_omitted() {
        let resp = TokenResponse {
            access_token: "new-access".into(),
            refresh_token: None,
            expires_in: Some(3600),
        };
        let tokens = to_cal_tokens(resp, Some("old-refresh"), "e@x.com".into(), 1000);
        assert_eq!(tokens.access, "new-access");
        assert_eq!(tokens.refresh, "old-refresh", "prior refresh preserved");
        assert_eq!(tokens.expires_at_epoch, 4600);
        assert_eq!(tokens.email, "e@x.com");
    }

    #[test]
    fn to_cal_tokens_prefers_new_refresh_when_present() {
        let resp = TokenResponse {
            access_token: "a".into(),
            refresh_token: Some("brand-new-refresh".into()),
            expires_in: None,
        };
        let tokens = to_cal_tokens(resp, Some("old"), String::new(), 0);
        assert_eq!(tokens.refresh, "brand-new-refresh");
        // Missing expires_in defaults to 1h.
        assert_eq!(tokens.expires_at_epoch, 3600);
    }

    #[tokio::test]
    async fn valid_access_token_returns_unexpired_without_network() {
        let store = MemoryCalStore::new();
        store
            .save(&CalTokens {
                access: "still-good".into(),
                refresh: "r".into(),
                expires_at_epoch: 10_000,
                email: "e@x.com".into(),
            })
            .unwrap();
        let cfg = configured(Provider::Google);
        // now well before expiry (minus skew) → returns cached, no HTTP.
        let token = valid_access_token(&store, &cfg, 1_000).await.unwrap();
        assert_eq!(token, "still-good");
    }

    #[tokio::test]
    async fn valid_access_token_errors_when_no_tokens() {
        let store = MemoryCalStore::new();
        let cfg = configured(Provider::Microsoft);
        let err = valid_access_token(&store, &cfg, 1_000).await.unwrap_err();
        assert!(err.to_string().contains("no calendar tokens"));
    }

    #[tokio::test]
    async fn valid_access_token_errors_when_expired_without_refresh() {
        let store = MemoryCalStore::new();
        store
            .save(&CalTokens {
                access: "stale".into(),
                refresh: "   ".into(), // blank refresh
                expires_at_epoch: 100,
                email: String::new(),
            })
            .unwrap();
        let cfg = configured(Provider::Google);
        let err = valid_access_token(&store, &cfg, 1_000).await.unwrap_err();
        assert!(err.to_string().contains("reconnect required"));
    }

    #[tokio::test]
    async fn serialized_access_token_refresh_rotates_only_once_for_concurrent_callers() {
        let response = r#"{
            "access_token":"fresh-access",
            "refresh_token":"rotated-refresh",
            "expires_in":1800
        }"#;
        let (token_url, server) = spawn_token_endpoint(200, response).await;
        let mut cfg = configured(Provider::Google);
        cfg.token_url = token_url;

        let store = std::sync::Arc::new(MemoryCalStore::new());
        store
            .save(&CalTokens {
                access: "expired-access".into(),
                refresh: "one-use-refresh".into(),
                expires_at_epoch: 100,
                email: "person@example.com".into(),
            })
            .unwrap();
        let operation = std::sync::Arc::new(tokio::sync::Mutex::new(()));

        let first = valid_access_token_serialized(store.as_ref(), &cfg, 1_000, operation.as_ref());
        let second = valid_access_token_serialized(store.as_ref(), &cfg, 1_000, operation.as_ref());
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.unwrap(), "fresh-access");
        assert_eq!(second.unwrap(), "fresh-access");

        let request = server.await.expect("one refresh request");
        assert!(request.body.contains("refresh_token=one-use-refresh"));
        let saved = store.load().unwrap().expect("rotated tokens persisted");
        assert_eq!(saved.refresh, "rotated-refresh");
    }

    #[tokio::test]
    async fn exchange_code_posts_the_pkce_authorization_form() {
        let response = r#"{
            "access_token":"fresh-access",
            "refresh_token":"fresh-refresh",
            "expires_in":1800
        }"#;
        let (token_url, server) = spawn_token_endpoint(200, response).await;
        let mut cfg = configured(Provider::Google);
        cfg.token_url = token_url;

        let tokens = exchange_code(
            &cfg,
            "http://127.0.0.1:49152/callback?source=bluey",
            "code +/=?",
            "verifier-._~",
        )
        .await
        .expect("authorization code exchange");
        assert_eq!(tokens.access_token, "fresh-access");
        assert_eq!(tokens.refresh_token.as_deref(), Some("fresh-refresh"));
        assert_eq!(tokens.expires_in, Some(1800));

        let request = server.await.expect("mock token endpoint task");
        assert!(request.head.starts_with("POST /token HTTP/1.1\r\n"));
        assert!(request
            .head
            .to_ascii_lowercase()
            .contains("content-type: application/x-www-form-urlencoded"));
        let form = url::form_urlencoded::parse(request.body.as_bytes())
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(form.len(), 5);
        assert_eq!(
            form.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
        assert_eq!(form.get("code").map(String::as_str), Some("code +/=?"));
        assert_eq!(
            form.get("redirect_uri").map(String::as_str),
            Some("http://127.0.0.1:49152/callback?source=bluey")
        );
        assert_eq!(
            form.get("client_id").map(String::as_str),
            Some("test-public-client.apps.googleusercontent.com")
        );
        assert_eq!(
            form.get("code_verifier").map(String::as_str),
            Some("verifier-._~")
        );
        assert!(!form.contains_key("client_secret"));
    }

    #[tokio::test]
    async fn exchange_code_surfaces_sanitized_http_failure_without_response_secrets() {
        let response = r#"{
            "error":"invalid_grant",
            "error_description":"Authorization code expired\nretry",
            "access_token":"must-not-leak"
        }"#;
        let (token_url, server) = spawn_token_endpoint(400, response).await;
        let mut cfg = configured(Provider::Microsoft);
        cfg.token_url = token_url;

        let error = exchange_code(&cfg, "http://localhost:49152", "expired-code", "verifier")
            .await
            .expect_err("HTTP 400 must fail the exchange");
        let message = error.to_string();
        assert!(message.contains("invalid_grant"));
        assert!(message.contains("HTTP 400 Bad Request"));
        assert!(message.contains("Authorization code expiredretry"));
        assert!(!message.contains('\n'));
        assert!(!message.contains("must-not-leak"));

        let request = server.await.expect("mock token endpoint task");
        let form = url::form_urlencoded::parse(request.body.as_bytes())
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(
            form.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
        assert_eq!(
            form.get("client_id").map(String::as_str),
            Some("00000000-0000-4000-8000-000000000000")
        );
    }

    #[test]
    fn connect_interactive_builds_a_valid_authorize_url() {
        // Verify the browser-open receives a well-formed authorize URL without
        // making any network call: we short-circuit the flow by panicking-free
        // capture of the URL and then aborting via a state that never arrives is
        // avoided — instead we assert on the pure URL builder the flow uses, so
        // this test stays hermetic (the loopback round-trip itself is covered in
        // `loopback.rs` tests, while the token POST is covered above).
        let cfg = Provider::Google.config();
        let url = build_authorize_url(&cfg, "http://127.0.0.1:5000", "chal", "st").unwrap();
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000"));
    }
}
