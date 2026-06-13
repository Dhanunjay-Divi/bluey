//! Vendor-agnostic HTTPS transport primitives for cloud agents.
//!
//! The point of this module: the per-vendor adapter (e.g.
//! [`super::anthropic`]) describes ITSELF as data — base URL, required
//! headers, auth model, endpoint paths — on the registry row. The generic
//! dispatcher reads that data and applies it uniformly. There is no
//! per-vendor branch in the dispatch path.
//!
//! Three primitives:
//!
//! - [`CloudTransport`] — the "what's the cloud shape of this agent?" enum
//!   that hangs off `AgentEntry`. `CloudTransport::None` for local CLI agents,
//!   `CloudTransport::Https {...}` for cloud agents.
//! - [`CloudAuth`] — how a vendor expects the credential delivered. The only
//!   variant today is [`CloudAuth::HeaderToken`], which covers Anthropic
//!   (`x-api-key`), OpenAI (`Authorization: Bearer …`), and any vendor that
//!   takes a static token in a header. New vendors (OAuth2 PKCE,
//!   per-org-JWT) add a new variant; existing rows are untouched.
//! - [`CloudEndpoints`] — vendor URL paths, keyed by purpose. Templates
//!   substitute `{session_id}` exactly once per call.

use std::time::Duration;

/// What HTTP shape an agent is driven through. Hangs off `AgentEntry`.
#[derive(Debug, Clone, Copy)]
pub enum CloudTransport {
    /// Local-CLI agent — no cloud transport. The drive layer uses
    /// [`crate::drive::cli`] instead.
    None,
    /// HTTPS to a vendor API. The dispatcher reads every field below as data
    /// and constructs the request — no `if vendor == "X"` branch anywhere.
    Https {
        /// Base URL (e.g. `"https://api.anthropic.com"`). No trailing slash.
        base_url: &'static str,
        /// Static headers required on every request — e.g. the Anthropic
        /// `anthropic-version: 2023-06-01` and `anthropic-beta:
        /// managed-agents-2026-04-01`. The beta header MUST live here, not in
        /// any code branch, so flipping it across vendors is a one-line data
        /// change.
        headers: CloudHeaders,
        /// How the credential is delivered.
        auth: CloudAuth,
        /// Endpoint paths, keyed by purpose.
        endpoints: CloudEndpoints,
    },
}

impl CloudTransport {
    /// Is this entry cloud-backed at all?
    pub fn is_cloud(&self) -> bool {
        matches!(self, CloudTransport::Https { .. })
    }

    /// Borrow the [`CloudAuth`], if any.
    pub fn auth(&self) -> Option<&CloudAuth> {
        match self {
            CloudTransport::Https { auth, .. } => Some(auth),
            CloudTransport::None => None,
        }
    }
}

/// Static headers carried on every request to a vendor's API. Stored as
/// `(name, value)` tuples so the registry row is `const`-friendly (no `String`s).
pub type CloudHeaders = &'static [(&'static str, &'static str)];

/// How a vendor expects the credential. Two shapes today:
///
/// - [`CloudAuth::HeaderToken`] — put the credential in an HTTP header,
///   optionally with a prefix. Covers Anthropic (`x-api-key`), OpenAI
///   (`Authorization: Bearer …`), and any vendor with a static token in a
///   header (Cursor Cloud's Bearer form too, see the cursor adapter).
/// - [`CloudAuth::BasicApiKey`] — `Authorization: Basic <base64(key:)>`,
///   the "API key as username with empty password" pattern Cursor's REST
///   surface accepts in addition to Bearer (compatible with `curl -u key:`).
///   Added for the cloud-vendor loop; left as a distinct variant so vendors
///   that REQUIRE basic auth (and reject Bearer for whatever reason) can
///   carry that promise on the registry row.
///
/// New auth schemes (OAuth2 PKCE, per-org JWT, AWS sigv4) are new variants.
/// Adding one is purely additive; existing rows are untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudAuth {
    /// Put the credential in an HTTP header, optionally with a prefix.
    ///
    /// Examples (data-driven; the adapter never names a vendor):
    /// - Anthropic Console API key: `header_name: "x-api-key", prefix: ""`.
    /// - OpenAI Bearer: `header_name: "Authorization", prefix: "Bearer "`.
    /// - Cursor Cloud Bearer: same shape as OpenAI.
    HeaderToken {
        header_name: &'static str,
        prefix: &'static str,
    },
    /// `Authorization: Basic <base64(api_key:)>`. The empty password
    /// (trailing colon) is significant — dropping it produces a different
    /// base64 string the server rejects.
    BasicApiKey,
}

impl CloudAuth {
    /// Apply this auth to a [`reqwest::RequestBuilder`]. Generic on top of
    /// `reqwest` so the dispatcher stays one code path.
    pub fn apply(
        &self,
        builder: reqwest::RequestBuilder,
        credential: &str,
    ) -> reqwest::RequestBuilder {
        match self {
            CloudAuth::HeaderToken {
                header_name,
                prefix,
            } => {
                let value = format!("{prefix}{credential}");
                builder.header(*header_name, value)
            }
            CloudAuth::BasicApiKey => {
                use base64::Engine as _;
                // The trailing colon (empty password) is REQUIRED: it is
                // base64 of "key:" not "key". Cursor's docs explicitly show
                // `curl -u $KEY:` and the server rejects the no-colon form.
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(format!("{credential}:"));
                builder.header("Authorization", format!("Basic {encoded}"))
            }
        }
    }
}

/// Endpoint paths a cloud vendor exposes, keyed by purpose. The dispatcher
/// joins them onto `base_url`. Templates use `{session_id}` or `{task_id}`
/// and are replaced verbatim — never URL-encoded (vendors return IDs that
/// are already URL-safe).
///
/// Every field is `Option` so a vendor can describe just the shape it
/// actually exposes:
/// - SESSION-shape vendors (Anthropic Managed Agents, Cursor's session API)
///   set `create_session/send_event/stream_events`.
/// - TASK-shape vendors (Codex Cloud's `POST /v1/codex/cloud/tasks`,
///   Copilot Coding Agent) set `create_task/get_task_status`.
///
/// An adapter only ever reads the fields its own shape needs.
#[derive(Debug, Clone, Copy, Default)]
pub struct CloudEndpoints {
    /// Create an agent definition (Anthropic-style). `None` for vendors that
    /// don't have a separate agent resource.
    pub create_agent: Option<&'static str>,
    /// Create an environment / sandbox config. `None` for vendors that don't
    /// have a separate environment resource.
    pub create_environment: Option<&'static str>,
    /// SESSION-shape: create a session.
    pub create_session: Option<&'static str>,
    /// SESSION-shape: send one or more user events to a session. Template
    /// carries `{session_id}`.
    pub send_event: Option<&'static str>,
    /// SESSION-shape: open the streaming response (SSE). Template carries
    /// `{session_id}`.
    pub stream_events: Option<&'static str>,
    /// SESSION-shape: delete a session permanently (release the sandbox).
    /// Template carries `{session_id}`. `None` for vendors that don't
    /// expose explicit delete.
    pub delete_session: Option<&'static str>,
    /// TASK-shape: create a background task that runs in a vendor-hosted
    /// sandbox and returns an artifact (PR, file diff, summary) minutes
    /// later. Used by Codex Cloud and any vendor whose unit of work is a
    /// fire-and-poll task, not a streaming session.
    pub create_task: Option<&'static str>,
    /// TASK-shape: read the status of a task by id. Template carries
    /// `{task_id}`.
    pub get_task_status: Option<&'static str>,
}

impl CloudEndpoints {
    /// Substitute `{session_id}` in a template path.
    pub fn render(template: &str, session_id: &str) -> String {
        template.replace("{session_id}", session_id)
    }

    /// Substitute `{task_id}` in a template path.
    pub fn render_task(template: &str, task_id: &str) -> String {
        template.replace("{task_id}", task_id)
    }
}

/// Build a `reqwest::Client` with sensible defaults for cloud agent traffic:
/// rustls TLS, 30s connect timeout, no overall request timeout (managed-agent
/// streams are long-running). Returns a fresh client each call — callers
/// should clone instead of reconstructing.
pub fn build_https_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT_STR)
        .build()
}

// ---- request/response helpers used by the cloud dispatcher path ---------
//
// The types and helpers below are additive to [`CloudTransport`] above. The
// `CloudTransport` enum tells the registry "what HTTP shape does this agent
// have" — it's a *schema*. [`CloudHttpsTransport`] is the *executor*: it
// loads the credential from the keychain, applies the [`CloudAuth`], fires
// the call, emits an audit line, and hands back the body for adapter parsing.
//
// They co-exist so the schema and the executor can evolve independently
// (e.g. a new auth scheme adds a [`CloudAuth`] variant; a new executor
// feature — streaming SSE, retries — bolts onto the dispatcher without
// touching the registry rows).

use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE, USER_AGENT};

use super::audit::{emit as audit_emit, AuditEvent};
// Re-exported so adapter modules can write
// `use super::transport::{CloudAuth, ..., VendorCredentialStore}` in one
// import line — keychain types are part of the transport's public surface
// since you can't call `send()` without one.
pub use super::keychain::VendorCredentialStore;

/// Default per-call timeout for cloud vendor HTTP calls. Streaming endpoints
/// (SSE) override this on a per-call basis since they intentionally hold the
/// connection open across the full agent run.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// User-Agent we identify Bluey with. Vendors with abuse dashboards see this.
const USER_AGENT_STR: &str = concat!("bluey-agent-bridge/", env!("CARGO_PKG_VERSION"));

/// One built-but-not-yet-fired HTTP request. **Pure data** — the adapter
/// constructs this and unit-tests against the *exact* URL/body/headers
/// without spawning a real HTTPS call.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: reqwest::Method,
    pub url: String,
    /// Optional JSON body. Stored as a `serde_json::Value` so tests can
    /// assert on field-by-field equality; the transport serializes it once
    /// when the call is fired.
    pub json_body: Option<serde_json::Value>,
    /// Extra headers beyond the auto-applied ones (auth, content-type,
    /// user-agent). Adapter-controlled.
    pub extra_headers: Vec<(String, String)>,
    /// Per-call timeout override (None = [`DEFAULT_TIMEOUT`]).
    pub timeout: Option<Duration>,
}

impl HttpRequest {
    /// Construct a GET with no body.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: reqwest::Method::GET,
            url: url.into(),
            json_body: None,
            extra_headers: Vec::new(),
            timeout: None,
        }
    }

    /// Construct a POST with a JSON body.
    pub fn post_json(url: impl Into<String>, body: serde_json::Value) -> Self {
        Self {
            method: reqwest::Method::POST,
            url: url.into(),
            json_body: Some(body),
            extra_headers: Vec::new(),
            timeout: None,
        }
    }

    /// Add an extra header. Chained-builder style; returns self for ergonomic
    /// inline construction in adapters.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_headers.push((name.into(), value.into()));
        self
    }
}

/// One received HTTP response after the transport has fired the request.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    /// Body bytes verbatim. Adapter parses (the transport is shape-agnostic).
    pub body: Vec<u8>,
    /// `True` iff the response carried any of the well-known correlation-id
    /// headers (`x-request-id`, `x-cursor-request-id`, etc.). The id itself
    /// is NOT retained — the audit log only records presence.
    pub request_id_present: bool,
}

impl HttpResponse {
    /// Decode the body as UTF-8 text, lossy-replacing invalid bytes. Used by
    /// adapters to construct error messages from vendor error envelopes.
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// Convenience wrappers for the two most-common header-token shapes. Adapters
/// that don't want to spell out the `CloudAuth::HeaderToken { … }` literal
/// every time can use these.
pub const BEARER_AUTH: CloudAuth = CloudAuth::HeaderToken {
    header_name: "Authorization",
    prefix: "Bearer ",
};

/// Cursor's `-u key:` form, equivalent to `Authorization: Basic <base64(key:)>`.
pub const BASIC_API_KEY_AUTH: CloudAuth = CloudAuth::BasicApiKey;

/// Re-export under the names that read most naturally in adapter code.
/// These are zero-sized types because they don't carry state — they merely
/// pick a `CloudAuth` variant.
#[derive(Debug, Clone, Copy, Default)]
pub struct BearerAuth;

impl BearerAuth {
    /// The corresponding [`CloudAuth`] variant.
    pub const AUTH: CloudAuth = BEARER_AUTH;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BasicAuthApiKey;

impl BasicAuthApiKey {
    /// The corresponding [`CloudAuth`] variant.
    pub const AUTH: CloudAuth = BASIC_API_KEY_AUTH;
}

/// HTTPS dispatcher / executor over [`reqwest`]. One transport instance is
/// reusable across many calls / many vendors — each call carries its own
/// auth + credential.
///
/// The transport never holds a credential. It is fetched from the supplied
/// [`VendorCredentialStore`] at call time and dropped before the response is
/// returned, so a panic mid-call cannot leave the credential in a heap state
/// the next call would observe.
pub struct CloudHttpsTransport {
    client: reqwest::Client,
}

impl Default for CloudHttpsTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudHttpsTransport {
    /// Construct with the same defaults as [`build_https_client`].
    pub fn new() -> Self {
        let client = build_https_client().expect("reqwest client builder infallible here");
        Self { client }
    }

    /// Fire one HTTP request. Loads the credential from `creds` (key
    /// `credential_key`, typically `"api_key"`), applies the auth header
    /// using `auth`, sends, and emits one audit line under `vendor` for the
    /// `endpoint` path.
    ///
    /// `endpoint` is a stable display string (e.g. `"/v1/agents"`) used
    /// purely for the audit log — it must NOT be derived from user input.
    pub async fn send(
        &self,
        vendor: &'static str,
        endpoint: &'static str,
        request: &HttpRequest,
        auth: CloudAuth,
        creds: &dyn VendorCredentialStore,
        credential_key: &str,
    ) -> Result<HttpResponse> {
        let credential = creds
            .load(credential_key)?
            .with_context(|| format!("{vendor} credential not present (key {credential_key})"))?;

        let timeout = request.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let mut builder = self
            .client
            .request(request.method.clone(), &request.url)
            .timeout(timeout);

        builder = auth.apply(builder, &credential);
        // Drop the plaintext credential immediately after the header has
        // been applied. `auth.apply` consumed it into the request header.
        drop(credential);
        builder = builder.header(USER_AGENT, USER_AGENT_STR);
        if request.json_body.is_some() {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        for (name, value) in &request.extra_headers {
            let name = HeaderName::try_from(name.as_str())
                .with_context(|| format!("illegal header name {name:?}"))?;
            let value = HeaderValue::from_str(value)
                .with_context(|| format!("illegal value for header {name}"))?;
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.json_body {
            builder = builder.json(body);
        }

        let method_str = method_static_str(&request.method);
        let started = Instant::now();
        let result = builder.send().await;
        let latency = started.elapsed();

        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                let request_id_present = correlation_id_present(response.headers());
                let body = response
                    .bytes()
                    .await
                    .context("reading cloud response body")?
                    .to_vec();

                audit_emit(&AuditEvent::new(
                    vendor,
                    endpoint,
                    method_str,
                    Some(status),
                    latency,
                    request_id_present,
                ));

                Ok(HttpResponse {
                    status,
                    body,
                    request_id_present,
                })
            }
            Err(e) => {
                audit_emit(&AuditEvent::new(
                    vendor, endpoint, method_str, None, latency, false,
                ));
                Err(anyhow::Error::new(e).context(format!(
                    "{vendor} {method_str} {endpoint} failed before status"
                )))
            }
        }
    }

    /// Build (but do not fire) the request, returning the underlying
    /// `reqwest::Request` for shape assertions in unit tests. Useful for
    /// asserting "the auth header has the expected shape, and the URL/body
    /// match the docs" without spawning a real HTTPS call.
    pub fn request_for(
        &self,
        vendor: &'static str,
        request: &HttpRequest,
        auth: CloudAuth,
        creds: &dyn VendorCredentialStore,
        credential_key: &str,
    ) -> Result<reqwest::Request> {
        let credential = creds
            .load(credential_key)?
            .with_context(|| format!("{vendor} credential not present (key {credential_key})"))?;
        let timeout = request.timeout.unwrap_or(DEFAULT_TIMEOUT);
        let mut builder = self
            .client
            .request(request.method.clone(), &request.url)
            .timeout(timeout);
        builder = auth.apply(builder, &credential);
        drop(credential);
        builder = builder.header(USER_AGENT, USER_AGENT_STR);
        if request.json_body.is_some() {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        for (name, value) in &request.extra_headers {
            let name = HeaderName::try_from(name.as_str())
                .with_context(|| format!("illegal header name {name:?}"))?;
            let value = HeaderValue::from_str(value)
                .with_context(|| format!("illegal value for header {name}"))?;
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.json_body {
            builder = builder.json(body);
        }
        builder.build().context("building request for inspection")
    }
}

/// Map a `reqwest::Method` to one of a fixed set of `&'static str` labels
/// for the audit log (the audit struct holds `&'static`, so we cannot
/// allocate per-call).
fn method_static_str(m: &reqwest::Method) -> &'static str {
    if m == reqwest::Method::GET {
        "GET"
    } else if m == reqwest::Method::POST {
        "POST"
    } else if m == reqwest::Method::PUT {
        "PUT"
    } else if m == reqwest::Method::DELETE {
        "DELETE"
    } else if m == reqwest::Method::PATCH {
        "PATCH"
    } else {
        "OTHER"
    }
}

/// Check the response headers for any of the well-known vendor correlation-
/// id header names. We only record *presence* (a boolean), never the value.
fn correlation_id_present(headers: &HeaderMap) -> bool {
    const NAMES: &[&str] = &[
        "x-request-id",
        "x-correlation-id",
        "x-cursor-request-id",
        // GitHub's correlation header — every REST + Copilot Coding Agent
        // response carries this; used by GitHub support to trace a call.
        "x-github-request-id",
        "request-id",
    ];
    NAMES.iter().any(|n| headers.contains_key(*n))
}

#[cfg(test)]
mod tests {
    use super::*;
    // `decode` is a trait method on `base64::Engine`; the trait MUST be in
    // scope at the call site even though `STANDARD` is reachable as a path.
    use base64::Engine as _;

    // ---- header-token auth ------------------------------------------------

    #[test]
    fn api_key_auth_sets_x_api_key_header() {
        // Anthropic-shape auth: header = "x-api-key", no prefix.
        // The credential becomes the entire header value.
        let auth = CloudAuth::HeaderToken {
            header_name: "x-api-key",
            prefix: "",
        };
        let client = reqwest::Client::new();
        let req = auth
            .apply(client.get("http://localhost/"), "sk-ant-api03-secret")
            .build()
            .expect("build request");
        let value = req
            .headers()
            .get("x-api-key")
            .expect("x-api-key header set")
            .to_str()
            .unwrap();
        assert_eq!(value, "sk-ant-api03-secret");
    }

    #[test]
    fn bearer_auth_sets_authorization_header_with_prefix() {
        // OpenAI-shape auth: header = "Authorization", prefix = "Bearer ".
        // Same generic primitive, no per-vendor branch.
        let auth = CloudAuth::HeaderToken {
            header_name: "Authorization",
            prefix: "Bearer ",
        };
        let client = reqwest::Client::new();
        let req = auth
            .apply(client.get("http://localhost/"), "sk-openai-xyz")
            .build()
            .expect("build request");
        let value = req
            .headers()
            .get("Authorization")
            .expect("Authorization header set")
            .to_str()
            .unwrap();
        assert_eq!(value, "Bearer sk-openai-xyz");
    }

    // ---- beta header is data ----------------------------------------------

    #[test]
    fn beta_header_is_data_driven() {
        // The registry row owns the headers slice; the dispatcher iterates it
        // generically. Test the iteration shape: a row carrying the Anthropic
        // beta header must produce it on every request the dispatcher builds.
        let headers: CloudHeaders = &[
            ("anthropic-version", "2023-06-01"),
            ("anthropic-beta", "managed-agents-2026-04-01"),
        ];
        let client = reqwest::Client::new();
        let mut builder = client.get("http://localhost/");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let req = builder.build().expect("build request");
        assert_eq!(
            req.headers()
                .get("anthropic-beta")
                .and_then(|v| v.to_str().ok()),
            Some("managed-agents-2026-04-01"),
            "the beta header MUST be carried as data, never hardcoded by vendor name",
        );
        assert_eq!(
            req.headers()
                .get("anthropic-version")
                .and_then(|v| v.to_str().ok()),
            Some("2023-06-01"),
        );
    }

    // ---- endpoint templating ----------------------------------------------

    #[test]
    fn endpoint_template_substitutes_session_id() {
        let rendered = CloudEndpoints::render("/v1/sessions/{session_id}/events", "sess-abc-123");
        assert_eq!(rendered, "/v1/sessions/sess-abc-123/events");
    }

    #[test]
    fn endpoint_template_is_inert_without_marker() {
        let rendered = CloudEndpoints::render("/v1/agents", "anything");
        assert_eq!(rendered, "/v1/agents");
    }

    // ---- transport classification ----------------------------------------

    #[test]
    fn cloud_transport_none_is_not_cloud() {
        assert!(!CloudTransport::None.is_cloud());
        assert!(CloudTransport::None.auth().is_none());
    }

    #[test]
    fn cloud_transport_https_is_cloud() {
        let t = CloudTransport::Https {
            base_url: "https://api.example.com",
            headers: &[],
            auth: CloudAuth::HeaderToken {
                header_name: "x-api-key",
                prefix: "",
            },
            endpoints: CloudEndpoints {
                create_agent: None,
                create_environment: None,
                create_session: Some("/v1/sessions"),
                send_event: Some("/v1/sessions/{session_id}/events"),
                stream_events: Some("/v1/sessions/{session_id}/stream"),
                delete_session: None,
                create_task: None,
                get_task_status: None,
            },
        };
        assert!(t.is_cloud());
        assert!(t.auth().is_some());
    }

    // ---- basic-api-key auth (Cursor's `-u key:` form) --------------------

    #[test]
    fn basic_api_key_auth_uses_trailing_colon() {
        // Cursor docs explicitly use `-u $CURSOR_API_KEY:` (note the empty
        // password). That is base64 of "key:" — NOT "key" — and the
        // distinction matters: dropping the colon produces a different
        // base64 string that the server rejects.
        let auth = CloudAuth::BasicApiKey;
        let client = reqwest::Client::new();
        let req = auth
            .apply(client.get("http://localhost/"), "crsr_abc")
            .build()
            .expect("build request");
        let raw = req
            .headers()
            .get("Authorization")
            .expect("Authorization header set")
            .to_str()
            .unwrap();
        assert!(raw.starts_with("Basic "));
        let b64 = raw.trim_start_matches("Basic ");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert_eq!(
            decoded_str, "crsr_abc:",
            "must encode key with trailing colon, empty password"
        );
    }

    #[test]
    fn bearer_auth_const_is_authorization_bearer() {
        // The `BearerAuth::AUTH` constant maps to a HeaderToken pointing at
        // `Authorization` with the `Bearer ` prefix. Both Cursor and any
        // other Bearer-style vendor can use it without spelling out the
        // enum literal.
        let auth = BearerAuth::AUTH;
        assert_eq!(
            auth,
            CloudAuth::HeaderToken {
                header_name: "Authorization",
                prefix: "Bearer ",
            }
        );
    }

    // ---- request/response helpers ----------------------------------------

    fn store_with_token(
        vendor: &'static str,
        token: &str,
    ) -> crate::cloud::keychain::MemoryCredentialStore {
        let s = crate::cloud::keychain::MemoryCredentialStore::new(vendor);
        s.save("api_key", token).expect("save");
        s
    }

    #[test]
    fn request_for_applies_bearer_header() {
        let transport = CloudHttpsTransport::new();
        let creds = store_with_token("cursor", "crsr_top_secret");
        let req = HttpRequest::get("https://api.cursor.com/v1/me");
        let built = transport
            .request_for("cursor", &req, BearerAuth::AUTH, &creds, "api_key")
            .unwrap();
        let auth_h = built
            .headers()
            .get("Authorization")
            .expect("auth header set");
        assert_eq!(auth_h.to_str().unwrap(), "Bearer crsr_top_secret");
        let ua = built
            .headers()
            .get(reqwest::header::USER_AGENT)
            .expect("ua header");
        assert!(ua.to_str().unwrap().starts_with("bluey-agent-bridge/"));
    }

    #[test]
    fn request_for_applies_basic_header_with_trailing_colon() {
        let transport = CloudHttpsTransport::new();
        let creds = store_with_token("cursor", "crsr_xyz");
        let req = HttpRequest::get("https://api.cursor.com/v1/me");
        let built = transport
            .request_for("cursor", &req, BasicAuthApiKey::AUTH, &creds, "api_key")
            .unwrap();
        let raw = built
            .headers()
            .get("Authorization")
            .expect("auth header")
            .to_str()
            .unwrap();
        let b64 = raw.trim_start_matches("Basic ");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "crsr_xyz:");
    }

    #[test]
    fn request_for_without_credential_errors_clearly() {
        let transport = CloudHttpsTransport::new();
        let creds = crate::cloud::keychain::MemoryCredentialStore::new("cursor");
        let req = HttpRequest::get("https://api.cursor.com/v1/me");
        let err = transport
            .request_for("cursor", &req, BearerAuth::AUTH, &creds, "api_key")
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("cursor"));
        assert!(msg.contains("credential not present"));
    }

    #[test]
    fn http_request_post_json_sets_content_type() {
        let transport = CloudHttpsTransport::new();
        let creds = store_with_token("cursor", "crsr_xyz");
        let body = serde_json::json!({ "prompt": { "text": "hi", "images": [] } });
        let req = HttpRequest::post_json("https://api.cursor.com/v1/agents", body);
        let built = transport
            .request_for("cursor", &req, BearerAuth::AUTH, &creds, "api_key")
            .unwrap();
        assert_eq!(
            built
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        assert!(built.body().is_some());
    }

    #[test]
    fn http_request_extra_headers_are_applied() {
        let transport = CloudHttpsTransport::new();
        let creds = store_with_token("cursor", "crsr_xyz");
        let req = HttpRequest::get("https://api.cursor.com/v1/agents/bc/runs/run/stream")
            .with_header("Accept", "text/event-stream")
            .with_header("Last-Event-ID", "1713033000000-0");
        let built = transport
            .request_for("cursor", &req, BearerAuth::AUTH, &creds, "api_key")
            .unwrap();
        assert_eq!(
            built.headers().get("accept").unwrap().to_str().unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            built
                .headers()
                .get("last-event-id")
                .unwrap()
                .to_str()
                .unwrap(),
            "1713033000000-0"
        );
    }

    #[test]
    fn method_str_maps_known_methods() {
        assert_eq!(method_static_str(&reqwest::Method::GET), "GET");
        assert_eq!(method_static_str(&reqwest::Method::POST), "POST");
        assert_eq!(method_static_str(&reqwest::Method::DELETE), "DELETE");
        assert_eq!(
            method_static_str(&reqwest::Method::from_bytes(b"FOO").unwrap()),
            "OTHER"
        );
    }

    #[test]
    fn correlation_id_presence_detection() {
        let mut h = HeaderMap::new();
        assert!(!correlation_id_present(&h));
        h.insert("x-request-id", HeaderValue::from_static("abc-123"));
        assert!(correlation_id_present(&h));
        h.clear();
        h.insert("x-cursor-request-id", HeaderValue::from_static("xyz-789"));
        assert!(correlation_id_present(&h));
    }

    // ---- Wiremock round-trips: real send() against a mock server --------

    #[tokio::test]
    async fn send_round_trips_against_wiremock() {
        use wiremock::matchers::{header, header_exists, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/me"))
            .and(header_exists("authorization"))
            .and(header("authorization", "Bearer crsr_test_token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-request-id", "req-123")
                    .set_body_json(serde_json::json!({
                        "apiKeyName": "Test",
                        "userId": 42,
                    })),
            )
            .mount(&server)
            .await;

        let transport = CloudHttpsTransport::new();
        let creds = store_with_token("cursor", "crsr_test_token");
        let req = HttpRequest::get(format!("{}/v1/me", server.uri()));
        let resp = transport
            .send(
                "cursor",
                "/v1/me",
                &req,
                BearerAuth::AUTH,
                &creds,
                "api_key",
            )
            .await
            .expect("send");
        assert_eq!(resp.status, 200);
        assert!(resp.request_id_present);
        let body = resp.body_text();
        assert!(body.contains("Test"));
        // The response body must NEVER carry the token back to the caller;
        // wiremock didn't echo it, but assert anyway as a defense.
        assert!(!body.contains("crsr_test_token"));
    }

    #[tokio::test]
    async fn send_returns_4xx_for_adapter_to_parse() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/agents"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": "Forbidden",
                "message": "Cloud agents not available on the Free plan.",
            })))
            .mount(&server)
            .await;

        let transport = CloudHttpsTransport::new();
        let creds = store_with_token("cursor", "crsr_test_token");
        let req =
            HttpRequest::post_json(format!("{}/v1/agents", server.uri()), serde_json::json!({}));
        let resp = transport
            .send(
                "cursor",
                "/v1/agents",
                &req,
                BearerAuth::AUTH,
                &creds,
                "api_key",
            )
            .await
            .expect("send returns the response, not Err, on 4xx");
        assert_eq!(resp.status, 403);
        assert!(resp.body_text().contains("Free plan"));
    }
}
