//! HTTPS client for `bluey-server`. Handles auth-token attachment,
//! 401-retry-with-refresh, 402 → InsufficientBalance, 429 → RateLimited,
//! capacity-busy API errors → CapacityBusy.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{header, Client, Method, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use cue_core::{
    new_request_id, sanitize_interaction_id, trace_id_from_env, BLUEY_INTERACTION_ID_HEADER,
    BLUEY_REQUEST_ID_HEADER, BLUEY_TRACE_ID_HEADER,
};

use crate::{
    error::{Error, Result},
    tokens::{CredentialAuthority, CredentialSnapshot, TokenStore, Tokens},
    types::{
        AccountDeletionAck, AccountDeletionCapability, AccountDeletionStatus,
        AccountDeletionStatusRequest, ArtifactObjectResponse, AuthResponse, CloudSessionBundle,
        DeleteAccountRequest, EmbedBatchRequest, EmbedBatchResponse, EmbedRequest, EmbedResponse,
        InsufficientBalanceBody, RagQueryRequest, RagQueryResponse, SessionAuditBundleResponse,
        SessionListResponse, SttSessionCancelRequest, SttSessionCancelResponse, SttSessionRequest,
        SttSessionResponse, SupportDiagnosticConsentReceipt, SupportDiagnosticConsentRequest,
        SupportDiagnosticConsentStatus, SupportDiagnosticDeleteResponse, SyncBatchRequest,
        SyncBatchResponse,
    },
};

/// Configuration for the cloud client.
/// Default max retries for idempotent GETs (total attempts = 1 + this).
/// Override with BLUEY_CLOUD_GET_RETRIES.
const DEFAULT_GET_RETRY_MAX: u32 = 2;
/// Base backoff in milliseconds; doubles each attempt plus jitter.
/// Override with BLUEY_CLOUD_RETRY_BASE_MS.
const DEFAULT_RETRY_BASE_MS: u64 = 120;

fn get_retry_max() -> u32 {
    std::env::var("BLUEY_CLOUD_GET_RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_GET_RETRY_MAX)
}

fn retry_base_ms() -> u64 {
    std::env::var("BLUEY_CLOUD_RETRY_BASE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|ms| *ms > 0)
        .unwrap_or(DEFAULT_RETRY_BASE_MS)
}

/// Exponential backoff with bounded jitter. attempt 0 -> ~base, 1 -> ~2x base.
fn retry_backoff(attempt: u32) -> std::time::Duration {
    let base = retry_base_ms();
    let exp = base.saturating_mul(1u64 << attempt.min(4));
    // Cheap dependency-free jitter in [0, base/2): derived from the
    // monotonic-ish system nanos. Jitter only spreads retries; it does not
    // need to be cryptographic.
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.subsec_nanos() as u64) % (base / 2 + 1))
        .unwrap_or(0);
    std::time::Duration::from_millis(exp.saturating_add(jitter))
}

/// Only retry explicitly-transient conditions on idempotent GETs.
/// Network timeout/connect errors and gateway-class 5xx + 408.
/// NOT: Unauthorized, InsufficientBalance, RateLimited, CapacityBusy,
/// TrialEnded, 4xx, or Server 500/501 (ambiguous — may be a persistent bug).
fn is_retryable_get_error(error: &Error) -> bool {
    match error {
        Error::Network(e) => e.is_timeout() || e.is_connect(),
        Error::Server { status } => matches!(status, 502 | 503 | 504 | 408),
        _ => false,
    }
}

#[derive(Debug, Default, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    retry_after_secs: Option<u64>,
}

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
                .unwrap_or_else(|_| "https://bluey.sh".into()),
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
    cached: Arc<Mutex<CachedCredentials>>,
    interaction_id: Option<String>,
}

#[derive(Debug)]
struct CachedCredentials {
    generation: u64,
    bound_authority: Option<CredentialAuthority>,
    snapshot: Option<CredentialSnapshot>,
}

impl CachedCredentials {
    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.snapshot = None;
    }

    fn bind(&mut self, snapshot: CredentialSnapshot) {
        self.generation = self.generation.wrapping_add(1);
        self.bound_authority = Some(snapshot.authority().clone());
        self.snapshot = Some(snapshot);
    }

    fn replace_within_bound_authority(&mut self, snapshot: CredentialSnapshot) {
        self.generation = self.generation.wrapping_add(1);
        self.snapshot = Some(snapshot);
    }
}

impl CloudClient {
    pub fn new(config: ClientConfig, tokens: Arc<dyn TokenStore>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(&config.user_agent)
            .timeout(config.timeout)
            .build()?;
        let snapshot = tokens.load_snapshot()?;
        let cached = Arc::new(Mutex::new(CachedCredentials {
            generation: 0,
            bound_authority: snapshot
                .as_ref()
                .map(|snapshot| snapshot.authority().clone()),
            snapshot,
        }));
        Ok(Self {
            config,
            http,
            tokens,
            cached,
            interaction_id: None,
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
        let mut cached = self.cached.lock().unwrap();
        let previous = cached.snapshot.clone();
        cached.invalidate();
        if let Err(error) = self.tokens.save(&tokens) {
            if let Some(previous) = previous.as_ref() {
                let _ = self.tokens.clear_if_current(previous);
            }
            return Err(error);
        }
        let snapshot = self.tokens.load_snapshot()?.ok_or_else(|| {
            Error::TokenStore("credential store lost a completed token save".to_string())
        })?;
        if snapshot.tokens() != &tokens {
            return Err(Error::TokenStore(
                "credential store changed during token save".to_string(),
            ));
        }
        cached.bind(snapshot);
        Ok(())
    }

    /// Load tokens from cache (no I/O on the hot path).
    pub fn current_tokens(&self) -> Option<Tokens> {
        self.cached
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.tokens().clone())
    }

    /// Return the immutable, non-secret authority captured with the current
    /// credential pair. The owner and device fields must not be logged.
    pub fn credential_authority(&self) -> Option<CredentialAuthority> {
        self.cached
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.authority().clone())
    }

    /// Return the exact captured snapshot for in-process authority fencing.
    /// Its Debug implementation redacts all secrets.
    pub fn credential_snapshot(&self) -> Option<CredentialSnapshot> {
        self.cached.lock().unwrap().snapshot.clone()
    }

    /// Check that the persistent store still owns this exact credential
    /// snapshot. This performs store I/O and must not be used on hot audio or
    /// streaming chunk paths.
    pub fn credential_snapshot_is_current(&self, expected: &CredentialSnapshot) -> Result<bool> {
        Ok(self.tokens.load_snapshot()?.as_ref() == Some(expected))
    }

    /// Reload tokens from the persistent store into the in-memory cache.
    ///
    /// Long-running daemon tasks keep a `CloudClient` alive for hours. Browser
    /// login, CLI login, or a one-off CLI refresh can update the secure token
    /// store while that client still holds stale cached tokens. This gives those
    /// background tasks a safe recovery path without restarting Bluey.
    pub fn reload_tokens_from_store(&self) -> Result<bool> {
        let loaded = self.tokens.load_snapshot()?;
        let mut cached = self.cached.lock().unwrap();
        let Some(bound_authority) = cached.bound_authority.as_ref() else {
            cached.invalidate();
            return Ok(false);
        };
        let Some(loaded) = loaded else {
            cached.invalidate();
            return Ok(false);
        };
        if !bound_authority.same_profile_scope(loaded.authority()) {
            // A long-lived client belongs to the account/profile scope it was
            // created for. Never transplant a replacement login into it.
            cached.invalidate();
            return Ok(false);
        }
        cached.replace_within_bound_authority(loaded);
        Ok(true)
    }

    /// Return a copy of this client that attaches a fixed trace id to all
    /// outgoing HTTP calls.
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.config.trace_id = sanitize_interaction_id(&trace_id.into());
        self
    }

    /// Return a copy of this client that attaches one validated interaction
    /// UUID to all outgoing HTTP calls in the answer path.
    pub fn with_interaction_id(mut self, interaction_id: impl Into<String>) -> Self {
        self.interaction_id = sanitize_interaction_id(&interaction_id.into());
        self
    }

    /// Forget tokens in store + cache. Used by `bluey logout`.
    pub fn logout(&self) -> Result<()> {
        let mut cached = self.cached.lock().unwrap();
        cached.invalidate();
        cached.bound_authority = None;
        self.tokens.clear()?;
        Ok(())
    }

    /// Invalidate this client's captured bearer and clear persistent tokens
    /// only if they are still the exact pair this client observed.
    ///
    /// Returns false after another process logs in or refreshes the same
    /// account. The replacement credentials are never loaded into this client.
    pub fn clear_tokens_if_current(&self) -> Result<bool> {
        let expected = match self.credential_snapshot() {
            Some(snapshot) => snapshot,
            None => return Ok(false),
        };
        self.clear_credential_snapshot_if_current(&expected)
    }

    /// Clear only the exact credential snapshot captured by a delayed caller.
    ///
    /// Callers must capture `expected` before any prompt, network request, or
    /// other await boundary. Unlike [`Self::clear_tokens_if_current`], this
    /// method never substitutes a newer snapshot from this client's cache, so
    /// a late A1 response cannot clear refreshed A2 credentials.
    pub fn clear_credential_snapshot_if_current(
        &self,
        expected: &CredentialSnapshot,
    ) -> Result<bool> {
        let cleared = self.tokens.clear_if_current(expected)?;
        let mut cached = self.cached.lock().unwrap();
        if cached.snapshot.as_ref() == Some(expected) {
            cached.invalidate();
        }
        Ok(cleared)
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
    /// Authenticated GET with bounded retry-with-backoff on transient
    /// failures. GETs are idempotent by HTTP semantics, so retrying a
    /// balance/usage/session poll that hit a network blip or a 502/503/504
    /// is safe and removes spurious failures + balance flicker. Auth (401)
    /// refresh, 402, 429, and all 4xx are NOT retried here.
    pub async fn auth_get<Resp: DeserializeOwned>(&self, path: &str) -> Result<Resp> {
        let max = get_retry_max();
        let mut attempt: u32 = 0;
        loop {
            match self.auth_request(Method::GET, path, None::<&()>).await {
                Ok(value) => return Ok(value),
                Err(error) if attempt < max && is_retryable_get_error(&error) => {
                    tokio::time::sleep(retry_backoff(attempt)).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Authenticated POST. Auto-refresh on 401.
    pub async fn auth_post<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        self.auth_request(Method::POST, path, Some(body)).await
    }

    /// Begin or replay one capability-bound account deletion operation.
    ///
    /// The operation is idempotent on the server. If the authenticated response
    /// is lost, call [`Self::account_deletion_status`] with the same capability;
    /// an authorization error alone is never proof of deletion.
    pub async fn delete_account(
        &self,
        capability: &AccountDeletionCapability,
    ) -> Result<AccountDeletionAck> {
        self.auth_post(
            "/account/delete",
            &DeleteAccountRequest::confirmed(capability),
        )
        .await
    }

    /// Confirm a response-lost account deletion without reviving authentication.
    /// The server stores only a hash of the recovery token and returns no
    /// account identity or customer content.
    pub async fn account_deletion_status(
        &self,
        capability: &AccountDeletionCapability,
    ) -> Result<AccountDeletionStatus> {
        self.post_json(
            "/account/delete/status",
            &AccountDeletionStatusRequest::from(capability),
        )
        .await
    }

    pub async fn sync_batch(&self, batch: &SyncBatchRequest) -> Result<SyncBatchResponse> {
        self.auth_post("/sync/batch", batch).await
    }

    pub async fn list_cloud_sessions(&self, limit: Option<i64>) -> Result<SessionListResponse> {
        self.list_cloud_sessions_page(limit, None).await
    }

    pub async fn list_cloud_sessions_page(
        &self,
        limit: Option<i64>,
        cursor: Option<&str>,
    ) -> Result<SessionListResponse> {
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            if let Some(limit) = limit {
                query.append_pair("limit", &limit.clamp(1, 200).to_string());
            }
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
            query.finish()
        };
        let path = if query.is_empty() {
            "/sync/sessions".to_string()
        } else {
            format!("/sync/sessions?{query}")
        };
        self.auth_get(&path).await
    }

    pub async fn load_cloud_session(&self, session_id: &str) -> Result<CloudSessionBundle> {
        self.auth_get(&format!("/sync/sessions/{session_id}")).await
    }

    pub async fn delete_cloud_session(&self, session_id: &str) -> Result<SyncBatchResponse> {
        self.auth_request(
            Method::DELETE,
            &format!("/sync/sessions/{session_id}"),
            None::<&()>,
        )
        .await
    }

    pub async fn upload_artifact_object(
        &self,
        artifact_id: &str,
        session_id: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<ArtifactObjectResponse> {
        let path = format!("/sync/artifacts/{artifact_id}/object");
        let resp = self
            .send_bytes_with_auth(
                Method::POST,
                &path,
                bytes,
                content_type,
                &[("x-bluey-session-id", session_id)],
            )
            .await?;
        Self::parse_or_err(resp).await
    }

    pub async fn upload_session_audit_bundle(
        &self,
        session_id: &str,
        bundle_id: &str,
        bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<SessionAuditBundleResponse> {
        let path = format!("/sync/session-audit/{session_id}/{bundle_id}");
        let resp = self
            .send_bytes_with_auth(
                Method::POST,
                &path,
                bytes,
                content_type,
                &[
                    ("x-bluey-audit-schema-version", "1"),
                    ("x-bluey-content-policy", "metadata_only"),
                ],
            )
            .await?;
        Self::parse_or_err(resp).await
    }

    pub async fn get_support_diagnostic_consent(&self) -> Result<SupportDiagnosticConsentStatus> {
        self.auth_get("/sync/support-diagnostics/consent").await
    }

    pub async fn set_support_diagnostic_consent(
        &self,
        enabled: bool,
    ) -> Result<SupportDiagnosticConsentReceipt> {
        self.auth_request(
            Method::PUT,
            "/sync/support-diagnostics/consent",
            Some(&SupportDiagnosticConsentRequest::current(enabled)),
        )
        .await
    }

    pub async fn delete_all_support_diagnostics(&self) -> Result<SupportDiagnosticDeleteResponse> {
        self.auth_request(Method::DELETE, "/sync/support-diagnostics", None::<&()>)
            .await
    }

    pub async fn delete_session_support_diagnostics(
        &self,
        session_id: &str,
    ) -> Result<SupportDiagnosticDeleteResponse> {
        self.auth_request(
            Method::DELETE,
            &format!("/sync/support-diagnostics/{session_id}"),
            None::<&()>,
        )
        .await
    }

    pub async fn download_artifact_object(&self, artifact_id: &str) -> Result<Vec<u8>> {
        let path = format!("/sync/artifacts/{artifact_id}/object");
        let mut resp = self.send_with_auth(Method::GET, &path, None::<&()>).await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            if !self.refresh_tokens().await? {
                return Err(Error::Unauthorized);
            }
            resp = self.send_with_auth(Method::GET, &path, None::<&()>).await?;
        }
        let resp = Self::stream_or_err(resp).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn query_rag(&self, request: &RagQueryRequest) -> Result<RagQueryResponse> {
        self.auth_post("/rag/query", request).await
    }

    pub async fn embed(&self, request: &EmbedRequest) -> Result<EmbedResponse> {
        self.auth_post("/router/embed", request).await
    }

    pub async fn embed_batch(&self, request: &EmbedBatchRequest) -> Result<EmbedBatchResponse> {
        self.auth_post("/router/embed/batch", request).await
    }

    pub async fn create_stt_session(
        &self,
        request: &SttSessionRequest,
    ) -> Result<SttSessionResponse> {
        self.auth_post("/stt/session", request).await
    }

    pub async fn cancel_stt_session(
        &self,
        request: &SttSessionCancelRequest,
    ) -> Result<SttSessionCancelResponse> {
        self.auth_post("/stt/session/cancel", request).await
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

    async fn send_bytes_with_auth(
        &self,
        method: Method,
        path: &str,
        bytes: Vec<u8>,
        content_type: &str,
        headers: &[(&str, &str)],
    ) -> Result<Response> {
        let bytes = bytes::Bytes::from(bytes);
        let access = self.current_tokens().ok_or(Error::Unauthorized)?.access;
        let mut request = self
            .request_builder(method.clone(), path)
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .header(header::CONTENT_TYPE, content_type)
            .body(bytes.clone());
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let resp = request.send().await?;
        if resp.status() != StatusCode::UNAUTHORIZED {
            return Ok(resp);
        }
        if !self.refresh_tokens().await? {
            return Err(Error::Unauthorized);
        }
        let access = self.current_tokens().ok_or(Error::Unauthorized)?.access;
        let mut request = self
            .request_builder(method, path)
            .header(header::AUTHORIZATION, format!("Bearer {access}"))
            .header(header::CONTENT_TYPE, content_type)
            .body(bytes);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        Ok(request.send().await?)
    }

    fn request_builder(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .http
            .request(method, self.url(path))
            .header(BLUEY_REQUEST_ID_HEADER, new_request_id());
        if let Some(trace_id) = self.current_trace_id() {
            req = req.header(BLUEY_TRACE_ID_HEADER, trace_id);
        }
        if let Some(interaction_id) = self.interaction_id.as_deref() {
            req = req.header(BLUEY_INTERACTION_ID_HEADER, interaction_id);
        }
        req
    }

    fn current_trace_id(&self) -> Option<String> {
        self.config
            .trace_id
            .as_deref()
            .and_then(sanitize_interaction_id)
            .or_else(trace_id_from_env)
    }

    /// Try to refresh the token pair. Returns true on success.
    async fn refresh_tokens(&self) -> Result<bool> {
        let (generation, current_snapshot) = match self.credential_snapshot_with_generation() {
            Some(snapshot) => snapshot,
            None => return Ok(false),
        };
        let cur = current_snapshot.tokens().clone();
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
        if auth.account.email.trim() != cur.email.trim() {
            return Err(Error::Other(
                "refresh response account identity did not match current credentials".to_string(),
            ));
        }
        let replacement = Tokens {
            access: auth.access_token,
            refresh: auth.refresh_token,
            email: cur.email.clone(),
        };

        let mut cached = self.cached.lock().unwrap();
        if cached.generation != generation || cached.snapshot.as_ref() != Some(&current_snapshot) {
            return Ok(false);
        }
        match self
            .tokens
            .compare_and_swap_snapshot(&current_snapshot, &replacement)
        {
            Ok(true) => {
                let stored = self.tokens.load_snapshot()?.ok_or_else(|| {
                    Error::TokenStore("credential store lost a completed token refresh".to_string())
                })?;
                if stored.tokens() != &replacement
                    || !current_snapshot
                        .authority()
                        .same_profile_scope(stored.authority())
                {
                    cached.invalidate();
                    return Ok(false);
                }
                cached.replace_within_bound_authority(stored);
                Ok(true)
            }
            Ok(false) => {
                // Another client/process changed the persistent account. Do
                // not retry the original request with that account's bearer.
                cached.invalidate();
                Ok(false)
            }
            Err(error) => {
                cached.invalidate();
                Err(error)
            }
        }
    }

    fn credential_snapshot_with_generation(&self) -> Option<(u64, CredentialSnapshot)> {
        let cached = self.cached.lock().unwrap();
        cached
            .snapshot
            .clone()
            .map(|snapshot| (cached.generation, snapshot))
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
                let retry_after_secs = retry_after_header(&resp);
                let body = resp.text().await.unwrap_or_default();
                if let Some(error) = capacity_busy_error(&body, retry_after_secs) {
                    return Err(error);
                }
                Err(Error::RateLimited {
                    retry_after_secs: retry_after_secs.unwrap_or(60),
                })
            }
            other => {
                let retry_after_secs = retry_after_header(&resp);
                let body = resp.text().await.unwrap_or_default();
                if other == StatusCode::BAD_REQUEST && is_internal_disclosure_blocked(&body) {
                    return Err(Error::InternalDisclosureBlocked);
                }
                if other == StatusCode::SERVICE_UNAVAILABLE {
                    if let Some(error) = capacity_busy_error(&body, retry_after_secs) {
                        return Err(error);
                    }
                }
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
    /// Unauthenticated GET with the same bounded transient retry as
    /// `auth_get` (idempotent, safe to retry on network/gateway blips).
    pub async fn public_get<Resp: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Resp> {
        let max = get_retry_max();
        let mut attempt: u32 = 0;
        loop {
            match self.public_get_once(path).await {
                Ok(value) => return Ok(value),
                Err(error) if attempt < max && is_retryable_get_error(&error) => {
                    tokio::time::sleep(retry_backoff(attempt)).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn public_get_once<Resp: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Resp> {
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
        self.logout()
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

fn retry_after_header(resp: &Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_value)
}

fn parse_retry_after_value(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return (seconds > 0).then_some(seconds);
    }
    let deadline = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let seconds = (deadline - Utc::now()).num_seconds();
    (seconds > 0).then_some(seconds as u64)
}

fn capacity_busy_error(body: &str, header_retry_after_secs: Option<u64>) -> Option<Error> {
    let parsed: ApiErrorBody = serde_json::from_str(body).ok()?;
    let reason = parsed.reason.unwrap_or_default();
    if !is_capacity_busy_reason(&reason) && parsed.retry_after_secs.is_none() {
        return None;
    }
    Some(Error::CapacityBusy {
        retry_after_secs: parsed
            .retry_after_secs
            .or(header_retry_after_secs)
            .unwrap_or(60)
            .max(1),
        reason: if reason.trim().is_empty() {
            "capacity_busy".to_string()
        } else {
            reason
        },
    })
}

fn is_internal_disclosure_blocked(body: &str) -> bool {
    serde_json::from_str::<ApiErrorBody>(body)
        .ok()
        .and_then(|body| body.reason)
        .is_some_and(|reason| reason == "internal_disclosure_blocked")
}

fn is_capacity_busy_reason(reason: &str) -> bool {
    matches!(
        reason,
        "provider_key_cooling_down"
            | "provider_capacity"
            | "upstream_spend_guard"
            | "capacity_busy"
    ) || reason.contains("capacity")
        || reason.contains("cooling")
}

pub(crate) fn log_safe_response_body(body: &str) -> String {
    if body.trim().is_empty() {
        return "<empty>".to_string();
    }
    let shape = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(serde_json::Value::Object(values)) => format!("json_object_fields={}", values.len()),
        Ok(serde_json::Value::Array(values)) => format!("json_array_items={}", values.len()),
        Ok(_) => "json_scalar".to_string(),
        Err(_) => "non_json".to_string(),
    };
    format!("<{shape};bytes={}>", body.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::MemoryStore;
    use wiremock::matchers::{header, header_exists, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TEST_INTERACTION_ID: &str = "550e8400-e29b-41d4-a716-446655440000";
    const TEST_TRACE_ID: &str = "550e8400-e29b-41d4-a716-446655440001";

    struct FailingReplacementStore {
        inner: Mutex<Option<Tokens>>,
    }

    impl FailingReplacementStore {
        fn with_tokens(tokens: Tokens) -> Self {
            Self {
                inner: Mutex::new(Some(tokens)),
            }
        }
    }

    impl TokenStore for FailingReplacementStore {
        fn save(&self, _tokens: &Tokens) -> Result<()> {
            Err(Error::TokenStore("injected save failure".to_string()))
        }

        fn load(&self) -> Result<Option<Tokens>> {
            Ok(self.inner.lock().unwrap().clone())
        }

        fn clear(&self) -> Result<()> {
            *self.inner.lock().unwrap() = None;
            Ok(())
        }

        fn clear_if_current(&self, expected: &CredentialSnapshot) -> Result<bool> {
            let mut current = self.inner.lock().unwrap();
            if current.as_ref() != Some(expected.tokens()) {
                return Ok(false);
            }
            *current = None;
            Ok(true)
        }

        fn compare_and_swap(&self, expected: &Tokens, replacement: &Tokens) -> Result<bool> {
            let mut current = self.inner.lock().unwrap();
            if current.as_ref() != Some(expected) {
                return Ok(false);
            }
            *current = Some(replacement.clone());
            Ok(true)
        }

        fn compare_and_swap_snapshot(
            &self,
            expected: &CredentialSnapshot,
            replacement: &Tokens,
        ) -> Result<bool> {
            self.compare_and_swap(expected.tokens(), replacement)
        }
    }

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

    async fn wait_for_request(server: &MockServer, request_path: &str) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if server
                    .received_requests()
                    .await
                    .unwrap()
                    .iter()
                    .any(|request| request.url.path() == request_path)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("request was not received before timeout");
    }

    #[tokio::test]
    async fn artifact_upload_sends_the_wire_parent_session() {
        let server = MockServer::start().await;
        let artifact_id = "61b8c310-27de-4cc1-b598-c62bdcc07ba8";
        let session_id = "session-parent-1";
        Mock::given(method("POST"))
            .and(path(format!("/sync/artifacts/{artifact_id}/object")))
            .and(header("authorization", "Bearer access"))
            .and(header("x-bluey-session-id", session_id))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "artifact_id": artifact_id,
                "session_id": session_id,
                "object_key": "objects/artifact",
                "size_bytes": 7,
                "sha256": "a".repeat(64),
                "content_type": "text/plain",
                "expires_at_ms": 1234
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "owner@example.test".into(),
            })
            .unwrap();

        let response = client
            .upload_artifact_object(artifact_id, session_id, b"payload".to_vec(), "text/plain")
            .await
            .unwrap();
        assert_eq!(response.session_id, session_id);
    }

    #[tokio::test]
    async fn session_list_page_encodes_and_returns_the_opaque_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sync/sessions"))
            .and(query_param("limit", "25"))
            .and(query_param("cursor", "opaque.cursor/with symbols"))
            .and(header("authorization", "Bearer access"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sessions": [],
                "deleted_sessions": [],
                "next_cursor": "next.opaque-cursor"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "owner@example.test".into(),
            })
            .unwrap();

        let response = client
            .list_cloud_sessions_page(Some(25), Some("opaque.cursor/with symbols"))
            .await
            .unwrap();
        assert_eq!(response.next_cursor.as_deref(), Some("next.opaque-cursor"));
    }

    #[test]
    fn session_list_response_accepts_legacy_payload_without_a_cursor() {
        let response: SessionListResponse = serde_json::from_value(serde_json::json!({
            "sessions": [],
            "deleted_sessions": []
        }))
        .unwrap();
        assert!(response.next_cursor.is_none());
    }

    #[tokio::test]
    async fn support_diagnostic_consent_and_cleanup_use_closed_authenticated_contracts() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/sync/support-diagnostics/consent"))
            .and(header("authorization", "Bearer access"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "receipt_id": "receipt-1",
                "enabled": true,
                "policy_version": crate::types::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
                "content_policy": crate::types::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
                "revision": 1,
                "recorded_at_ms": 1234
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/sync/support-diagnostics"))
            .and(header("authorization", "Bearer access"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "scope": "all",
                "session_id": null,
                "scheduled_objects": 2,
                "server_time_ms": 1235
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "owner@example.test".into(),
            })
            .unwrap();

        let receipt = client.set_support_diagnostic_consent(true).await.unwrap();
        assert!(receipt.enabled);
        let deleted = client.delete_all_support_diagnostics().await.unwrap();
        assert_eq!(deleted.scheduled_objects, 2);

        let requests = server.received_requests().await.unwrap();
        let consent_request = requests
            .iter()
            .find(|request| request.url.path() == "/sync/support-diagnostics/consent")
            .expect("consent request");
        let body: serde_json::Value =
            serde_json::from_slice(&consent_request.body).expect("consent JSON");
        assert_eq!(
            body,
            serde_json::json!({
                "enabled": true,
                "policy_version": crate::types::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
                "content_policy": crate::types::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            })
        );
    }

    #[tokio::test]
    async fn support_diagnostic_upload_sends_schema_and_content_policy_headers() {
        let server = MockServer::start().await;
        let session_id = "550e8400-e29b-41d4-a716-446655440000";
        let bundle_id = "diagnostic-test";
        Mock::given(method("POST"))
            .and(path(format!(
                "/sync/session-audit/{session_id}/{bundle_id}"
            )))
            .and(header("authorization", "Bearer access"))
            .and(header("content-type", "application/json"))
            .and(header("x-bluey-audit-schema-version", "1"))
            .and(header("x-bluey-content-policy", "metadata_only"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "session_id": session_id,
                "bundle_id": bundle_id,
                "object_key": "support/object",
                "size_bytes": 2,
                "sha256": "a".repeat(64),
                "content_type": "application/json",
                "expires_at_ms": 1234
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "owner@example.test".into(),
            })
            .unwrap();
        client
            .upload_session_audit_bundle(session_id, bundle_id, b"{}".to_vec(), "application/json")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn account_deletion_reuses_exact_operation_and_status_is_capability_only() {
        let server = MockServer::start().await;
        let capability = AccountDeletionCapability {
            operation_id: "550e8400-e29b-41d4-a716-446655440401".to_string(),
            recovery_token: "550e8400-e29b-41d4-a716-446655440402".to_string(),
        };
        Mock::given(method("POST"))
            .and(path("/account/delete"))
            .and(header("authorization", "Bearer access"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "deleted": true,
                "deleted_at": "2026-08-30T00:00:00Z",
                "deletion_pending": false,
                "retry_after_ms": null,
                "object_count_deleted": 0,
                "note": "deleted"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/account/delete/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "deleted": true,
                "deletion_pending": false,
                "deleted_at": "2026-08-30T00:00:00Z",
                "expires_at_ms": 1
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri());
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "owner@example.test".into(),
            })
            .unwrap();
        assert!(client.delete_account(&capability).await.unwrap().deleted);
        assert!(
            client
                .account_deletion_status(&capability)
                .await
                .unwrap()
                .deleted
        );

        let requests = server.received_requests().await.unwrap();
        for request in requests {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["operation_id"], capability.operation_id);
            assert_eq!(body["recovery_token"], capability.recovery_token);
            if request.url.path() == "/account/delete/status" {
                assert!(!request.headers.contains_key("authorization"));
            }
        }
    }

    #[test]
    fn is_retryable_get_error_classifies_transient_only() {
        // Gateway-class 5xx + 408 are transient -> retry.
        for status in [502u16, 503, 504, 408] {
            assert!(
                is_retryable_get_error(&Error::Server { status }),
                "status {status} should be retryable"
            );
        }
        // Ambiguous / client / auth / billing errors are NOT retried.
        for status in [400u16, 401, 403, 404, 409, 422, 500, 501] {
            assert!(
                !is_retryable_get_error(&Error::Server { status }),
                "status {status} must not be retryable"
            );
        }
        assert!(!is_retryable_get_error(&Error::Unauthorized));
        assert!(!is_retryable_get_error(&Error::RateLimited {
            retry_after_secs: 5
        }));
        assert!(!is_retryable_get_error(&Error::CapacityBusy {
            retry_after_secs: 5,
            reason: "provider_key_cooling_down".into(),
        }));
        assert!(!is_retryable_get_error(&Error::TrialEnded));
        assert!(!is_retryable_get_error(&Error::Other("x".into())));
    }

    #[test]
    fn retry_backoff_increases_and_is_bounded() {
        let base = retry_base_ms();
        let d0 = retry_backoff(0).as_millis() as u64;
        let d1 = retry_backoff(1).as_millis() as u64;
        // attempt 0 >= base; attempt 1 >= 2x base (jitter only adds).
        assert!(d0 >= base, "d0={d0} base={base}");
        assert!(d1 >= base * 2, "d1={d1} base={base}");
        // Bounded: jitter is < base/2 above the exponential term.
        assert!(d0 < base * 2, "d0={d0} should stay under 2x base");
    }

    #[test]
    fn reload_tokens_from_store_accepts_same_authority_refresh() {
        let config = ClientConfig {
            base_url: "https://bluey.test".to_string(),
            user_agent: "test".into(),
            timeout: Duration::from_secs(10),
            trace_id: None,
        };
        let store = Arc::new(MemoryStore::new());
        TokenStore::save(
            store.as_ref(),
            &Tokens {
                access: "old-access".to_string(),
                refresh: "old-refresh".to_string(),
                email: "owner@example.com".to_string(),
            },
        )
        .unwrap();
        let client = CloudClient::new(config, store.clone()).unwrap();
        assert_eq!(
            client.current_tokens().unwrap().access.as_str(),
            "old-access"
        );

        TokenStore::save(
            store.as_ref(),
            &Tokens {
                access: "new-access".to_string(),
                refresh: "new-refresh".to_string(),
                email: "owner@example.com".to_string(),
            },
        )
        .unwrap();

        assert!(client.reload_tokens_from_store().unwrap());
        assert_eq!(
            client.current_tokens().unwrap().access.as_str(),
            "new-access"
        );
    }

    #[test]
    fn reload_tokens_from_store_rejects_replacement_authority() {
        let config = ClientConfig {
            base_url: "https://bluey.test".to_string(),
            user_agent: "test".into(),
            timeout: Duration::from_secs(10),
            trace_id: None,
        };
        let store = Arc::new(MemoryStore::new());
        let account_a = Tokens {
            access: "access-a".to_string(),
            refresh: "refresh-a".to_string(),
            email: "a@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_a).unwrap();
        let client = CloudClient::new(config, store.clone()).unwrap();

        let account_b = Tokens {
            access: "access-b".to_string(),
            refresh: "refresh-b".to_string(),
            email: "b@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_b).unwrap();

        assert!(!client.reload_tokens_from_store().unwrap());
        assert_eq!(client.current_tokens(), None);
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), Some(account_b));
    }

    #[test]
    fn conditional_client_clear_does_not_remove_replacement_or_refreshed_tokens() {
        let config = ClientConfig {
            base_url: "https://bluey.test".to_string(),
            user_agent: "test".into(),
            timeout: Duration::from_secs(10),
            trace_id: None,
        };
        let store = Arc::new(MemoryStore::new());
        let account_a1 = Tokens {
            access: "access-a1".to_string(),
            refresh: "refresh-a1".to_string(),
            email: "a@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_a1).unwrap();
        let stale_a1 = CloudClient::new(config.clone(), store.clone()).unwrap();
        let captured_a1 = stale_a1.credential_snapshot().unwrap();

        let account_a2 = Tokens {
            access: "access-a2".to_string(),
            refresh: "refresh-a2".to_string(),
            email: "a@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_a2).unwrap();
        assert!(stale_a1.reload_tokens_from_store().unwrap());
        assert!(!stale_a1
            .clear_credential_snapshot_if_current(&captured_a1)
            .unwrap());
        assert_eq!(stale_a1.current_tokens(), Some(account_a2.clone()));
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), Some(account_a2));

        let current_a2 = CloudClient::new(config.clone(), store.clone()).unwrap();
        let captured_a2 = current_a2.credential_snapshot().unwrap();
        let account_b = Tokens {
            access: "access-b".to_string(),
            refresh: "refresh-b".to_string(),
            email: "b@example.com".to_string(),
        };
        TokenStore::save(store.as_ref(), &account_b).unwrap();
        assert!(!current_a2
            .clear_credential_snapshot_if_current(&captured_a2)
            .unwrap());
        assert_eq!(
            TokenStore::load(store.as_ref()).unwrap(),
            Some(account_b.clone())
        );

        let current_b = CloudClient::new(config, store.clone()).unwrap();
        let captured_b = current_b.credential_snapshot().unwrap();
        assert!(current_b
            .clear_credential_snapshot_if_current(&captured_b)
            .unwrap());
        assert_eq!(TokenStore::load(store.as_ref()).unwrap(), None);
    }

    #[test]
    fn failed_account_switch_preserves_no_old_bearer() {
        let old = Tokens {
            access: "old-access".to_string(),
            refresh: "old-refresh".to_string(),
            email: "old@example.com".to_string(),
        };
        let store = Arc::new(FailingReplacementStore::with_tokens(old));
        let client = CloudClient::new(
            ClientConfig {
                base_url: "https://bluey.test".to_string(),
                user_agent: "test".to_string(),
                timeout: Duration::from_secs(10),
                trace_id: None,
            },
            store.clone(),
        )
        .unwrap();

        let error = client
            .save_tokens(Tokens {
                access: "new-access".to_string(),
                refresh: "new-refresh".to_string(),
                email: "new@example.com".to_string(),
            })
            .unwrap_err();

        assert!(matches!(error, Error::TokenStore(_)));
        assert_eq!(client.current_tokens(), None);
        assert_eq!(store.load().unwrap(), None);
    }

    #[tokio::test]
    async fn logout_invalidates_a_delayed_refresh_result() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header("authorization", "Bearer old-access"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/auth/refresh"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(250))
                    .set_body_json(serde_json::json!({
                        "access_token": "late-access",
                        "refresh_token": "late-refresh",
                        "expires_in": 3600,
                        "account": {
                            "id": "account-a",
                            "email": "a@example.com",
                            "balance_cents": 100,
                            "trial_seconds_remaining": 0
                        }
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let client = CloudClient::new(
            ClientConfig {
                base_url: server.uri(),
                user_agent: "test".to_string(),
                timeout: Duration::from_secs(10),
                trace_id: None,
            },
            store.clone(),
        )
        .unwrap();
        client
            .save_tokens(Tokens {
                access: "old-access".to_string(),
                refresh: "old-refresh".to_string(),
                email: "a@example.com".to_string(),
            })
            .unwrap();

        let request = tokio::spawn({
            let client = client.clone();
            async move { client.auth_get::<serde_json::Value>("/account/me").await }
        });
        wait_for_request(&server, "/auth/refresh").await;
        client.logout().unwrap();

        assert!(matches!(request.await.unwrap(), Err(Error::Unauthorized)));
        assert_eq!(client.current_tokens(), None);
        assert_eq!(store.load().unwrap(), None);
    }

    #[tokio::test]
    async fn account_switch_wins_over_a_delayed_refresh() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header("authorization", "Bearer old-access"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/auth/refresh"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(250))
                    .set_body_json(serde_json::json!({
                        "access_token": "late-access-a",
                        "refresh_token": "late-refresh-a",
                        "expires_in": 3600,
                        "account": {
                            "id": "account-a",
                            "email": "a@example.com",
                            "balance_cents": 100,
                            "trial_seconds_remaining": 0
                        }
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let store = Arc::new(MemoryStore::new());
        let client = CloudClient::new(
            ClientConfig {
                base_url: server.uri(),
                user_agent: "test".to_string(),
                timeout: Duration::from_secs(10),
                trace_id: None,
            },
            store.clone(),
        )
        .unwrap();
        client
            .save_tokens(Tokens {
                access: "old-access".to_string(),
                refresh: "old-refresh".to_string(),
                email: "a@example.com".to_string(),
            })
            .unwrap();

        let request = tokio::spawn({
            let client = client.clone();
            async move { client.auth_get::<serde_json::Value>("/account/me").await }
        });
        wait_for_request(&server, "/auth/refresh").await;
        let linked = Tokens {
            access: "access-b".to_string(),
            refresh: "refresh-b".to_string(),
            email: "b@example.com".to_string(),
        };
        client.save_tokens(linked.clone()).unwrap();

        assert!(matches!(request.await.unwrap(), Err(Error::Unauthorized)));
        assert_eq!(client.current_tokens(), Some(linked.clone()));
        assert_eq!(store.load().unwrap(), Some(linked));
    }

    #[tokio::test]
    async fn public_get_retries_persistent_transient_then_gives_up() {
        std::env::set_var("BLUEY_CLOUD_RETRY_BASE_MS", "1");
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pricing/tiers"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        let result: Result<serde_json::Value> = client.public_get("/pricing/tiers").await;
        std::env::remove_var("BLUEY_CLOUD_RETRY_BASE_MS");

        assert!(matches!(result, Err(Error::Server { status: 503 })));
        // 1 initial attempt + DEFAULT_GET_RETRY_MAX retries.
        let received = server.received_requests().await.unwrap();
        assert_eq!(
            received.len(),
            (1 + DEFAULT_GET_RETRY_MAX) as usize,
            "persistent 503 should be retried up to the bound"
        );
    }

    #[tokio::test]
    async fn public_get_does_not_retry_client_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pricing/tiers"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = client_for(server.uri());
        let result: Result<serde_json::Value> = client.public_get("/pricing/tiers").await;

        assert!(matches!(result, Err(Error::Server { status: 404 })));
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "4xx must not be retried");
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
                "reload_url": "https://bluey.sh/reload"
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

    #[tokio::test]
    async fn parse_or_err_maps_only_known_disclosure_reason_to_typed_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "request blocked",
                "reason": "internal_disclosure_blocked"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/router/other"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "arbitrary server detail",
                "reason": "unknown_reason"
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

        let typed: Result<serde_json::Value> = client
            .auth_post("/router/complete", &serde_json::json!({}))
            .await;
        assert!(matches!(typed, Err(Error::InternalDisclosureBlocked)));

        let bounded: Result<serde_json::Value> = client
            .auth_post("/router/other", &serde_json::json!({}))
            .await;
        assert!(matches!(bounded, Err(Error::Server { status: 400 })));
    }

    #[tokio::test]
    async fn stream_or_err_preserves_typed_disclosure_reason() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "request blocked",
                "reason": "internal_disclosure_blocked"
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
            .auth_post_stream("/router/complete/stream", &serde_json::json!({}))
            .await;

        assert!(matches!(result, Err(Error::InternalDisclosureBlocked)));
    }

    #[test]
    fn log_safe_response_body_records_shape_only() {
        let body = serde_json::json!({
            "error": "bad",
            "access_token": "secret-access",
            "refresh_token": "secret-refresh",
            "verification_url": "https://bluey.sh/link?token=secret",
            "nested": { "device_code": "device-secret", "nonce": "handoff-secret" }
        })
        .to_string();

        let safe = log_safe_response_body(&body);
        assert!(!safe.contains("secret-access"));
        assert!(!safe.contains("secret-refresh"));
        assert!(!safe.contains("device-secret"));
        assert!(!safe.contains("handoff-secret"));
        assert!(!safe.contains("https://bluey.sh/link"));
        assert!(safe.contains("json_object_fields=5"));
        assert!(safe.contains("bytes="));
        let non_json = log_safe_response_body("secret bearer response");
        assert!(!non_json.contains("secret bearer response"));
        assert!(non_json.contains("non_json"));
    }

    #[tokio::test]
    async fn parse_or_err_402_trial_ended_maps_to_trial_ended() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(ResponseTemplate::new(402).set_body_json(serde_json::json!({
                "balance_cents": 0,
                "reason": "trial_ended",
                "reload_url": "https://bluey.sh/reload"
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

    #[test]
    fn retry_after_parser_accepts_seconds_and_http_date() {
        assert_eq!(parse_retry_after_value("12"), Some(12));

        let future = (Utc::now() + chrono::Duration::seconds(30))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let parsed = parse_retry_after_value(&future).unwrap();
        assert!((1..=30).contains(&parsed));
    }

    #[tokio::test]
    async fn parse_or_err_429_capacity_body_maps_to_capacity_busy() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": "Bluey is handling a burst right now; retry shortly",
                "reason": "provider_key_cooling_down",
                "retry_after_secs": 17
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
        match r {
            Err(Error::CapacityBusy {
                retry_after_secs,
                reason,
            }) => {
                assert_eq!(retry_after_secs, 17);
                assert_eq!(reason, "provider_key_cooling_down");
            }
            other => panic!("expected CapacityBusy, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn parse_or_err_503_capacity_body_uses_retry_after_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete"))
            .respond_with(
                ResponseTemplate::new(503)
                    .insert_header("retry-after", "23")
                    .set_body_json(serde_json::json!({
                        "error": "Bluey is handling a burst right now; retry shortly",
                        "reason": "upstream_spend_guard"
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
            Err(Error::CapacityBusy {
                retry_after_secs,
                reason,
            }) => {
                assert_eq!(retry_after_secs, 23);
                assert_eq!(reason, "upstream_spend_guard");
            }
            other => panic!("expected CapacityBusy, got {other:?}"),
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
                "reload_url": "https://bluey.sh/reload"
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
    async fn auth_post_stream_maps_capacity_busy_before_streaming() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": "Bluey is handling a burst right now; retry shortly",
                "reason": "provider_capacity",
                "retry_after_secs": 31
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
            Err(Error::CapacityBusy {
                retry_after_secs,
                reason,
            }) => {
                assert_eq!(retry_after_secs, 31);
                assert_eq!(reason, "provider_capacity");
            }
            other => panic!("expected CapacityBusy, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn auth_post_stream_refreshes_on_401_before_returning_stream() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .and(header("authorization", "Bearer old-access"))
            .and(header(BLUEY_INTERACTION_ID_HEADER, TEST_INTERACTION_ID))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/auth/refresh"))
            .and(header(BLUEY_INTERACTION_ID_HEADER, TEST_INTERACTION_ID))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new-access",
                "refresh_token": "new-refresh",
                "expires_in": 3600,
                "account": {
                    "id": "acct-1",
                    "email": "e@example.com",
                    "balance_cents": 100,
                    "trial_seconds_remaining": 0
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .and(header("authorization", "Bearer new-access"))
            .and(header(BLUEY_INTERACTION_ID_HEADER, TEST_INTERACTION_ID))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/x-ndjson")
                    .set_body_string("{\"type\":\"chunk\",\"text\":\"ok\"}\n"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri()).with_interaction_id(TEST_INTERACTION_ID);
        client
            .save_tokens(Tokens {
                access: "old-access".into(),
                refresh: "old-refresh".into(),
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

        assert_eq!(
            response.text().await.unwrap(),
            "{\"type\":\"chunk\",\"text\":\"ok\"}\n"
        );
        let refreshed = client.current_tokens().unwrap();
        assert_eq!(refreshed.access, "new-access");
        assert_eq!(refreshed.refresh, "new-refresh");
    }

    #[tokio::test]
    async fn auth_requests_send_trace_and_request_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/account/me"))
            .and(header(BLUEY_TRACE_ID_HEADER, TEST_TRACE_ID))
            .and(header(BLUEY_INTERACTION_ID_HEADER, TEST_INTERACTION_ID))
            .and(header_exists(BLUEY_REQUEST_ID_HEADER))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = client_for(server.uri())
            .with_trace_id(TEST_TRACE_ID)
            .with_interaction_id(TEST_INTERACTION_ID);
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

    #[tokio::test]
    async fn invalid_interaction_id_is_not_sent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pricing/tiers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tiers": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(server.uri()).with_interaction_id("person@example.com");
        let _: serde_json::Value = client.public_get("/pricing/tiers").await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0]
            .headers
            .contains_key(BLUEY_INTERACTION_ID_HEADER));
    }
}
