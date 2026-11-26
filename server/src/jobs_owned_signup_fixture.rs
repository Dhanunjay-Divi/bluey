//! Test-only authenticated candidate setup for the owned PostgreSQL Jobs fixture.

use anyhow::{Context, Result};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::EnvironmentRestore;

const SYNTHETIC_CANDIDATE: &str = "candidate@bluey-qa.test";
const SYNTHETIC_PASSWORD: &str = "Qa-0123456789abcdef0123456789abcdef0123456789abcdef";
const MAX_AUTH_BODY_BYTES: usize = 64 * 1024;

pub(super) struct SignedUpCandidate {
    pub account_id: String,
    pub access_token: String,
}

/// Closed fixture configuration. Callers may explicitly layer a loopback-only
/// transport or object store, but this default has neither provider authority.
pub(super) fn base_config(database_url: &str, jwt_secret: &str) -> crate::config::Config {
    crate::config::Config {
        port: 0,
        db_path: std::path::PathBuf::from(":memory:"),
        db_backend: crate::config::ServerDbBackend::Postgres,
        database_url: Some(database_url.to_string()),
        jwt_secret: jwt_secret.to_string(),
        public_url: "http://127.0.0.1".to_string(),
        stripe_secret_key: None,
        stripe_webhook_secret: None,
        upstream: crate::config::UpstreamKeys::default(),
        upstream_spend_guard: None,
        smtp: None,
        admin_emails: Vec::new(),
        trial_abuse: crate::config::TrialAbuseConfig::default(),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: false,
        object_storage: None,
        log_storage: None,
    }
}

pub(super) async fn signup_candidate(
    pool: &crate::db::DbPool,
    database_url: &str,
    jwt_secret: &str,
    email: &str,
) -> Result<SignedUpCandidate> {
    anyhow::ensure!(
        email == SYNTHETIC_CANDIDATE,
        "unexpected synthetic candidate"
    );
    anyhow::ensure!(
        crate::db::accounts::Account::fetch_by_email(pool, email)?.is_none(),
        "synthetic candidate already exists"
    );

    let mail = MockServer::start().await;
    anyhow::ensure!(
        mail.address().ip().is_loopback(),
        "mail mock must be loopback"
    );
    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header("Authorization", "Bearer fixture-resend-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "fixture-mail"})))
        .expect(1)
        .mount(&mail)
        .await;

    let _environment = EnvironmentRestore::install(&[
        ("BLUEY_MAIL_TRANSPORT", Some("resend".to_string())),
        ("BLUEY_RESEND_API_BASE_URL", Some(mail.uri())),
        ("HTTP_PROXY", None),
        ("http_proxy", None),
        ("HTTPS_PROXY", None),
        ("https_proxy", None),
        ("ALL_PROXY", None),
        ("all_proxy", None),
    ]);
    let mut config = base_config(database_url, jwt_secret);
    config.smtp = Some(crate::config::SmtpConfig {
        host: "127.0.0.1".to_string(),
        port: mail.address().port(),
        username: Some("resend".to_string()),
        password: Some("fixture-resend-api-key".to_string()),
        from: "Bluey <noreply@bluey.test>".to_string(),
        starttls: true,
    });
    let router = crate::api::build_router(pool.clone(), config);

    let start = router
        .clone()
        .oneshot(json_request(
            "/auth/signup/start",
            json!({
                "email": email,
                "password": SYNTHETIC_PASSWORD,
                "terms_accepted": true,
            }),
        )?)
        .await
        .context("send synthetic signup start")?;
    let start_body = bounded_json(start, "signup start").await?;
    anyhow::ensure!(
        start_body["email"].as_str() == Some(SYNTHETIC_CANDIDATE),
        "signup start candidate mismatch"
    );

    let mail_requests = mail.received_requests().await.context("read signup mail")?;
    anyhow::ensure!(mail_requests.len() == 1, "signup mail count mismatch");
    let mail_body: Value =
        serde_json::from_slice(&mail_requests[0].body).context("decode signup mail")?;
    anyhow::ensure!(
        mail_body["to"]
            .as_array()
            .is_some_and(|to| { to.len() == 1 && to[0].as_str() == Some(SYNTHETIC_CANDIDATE) })
            && mail_body["subject"].as_str() == Some("Your Bluey verification code"),
        "signup mail recipient mismatch"
    );
    let otp = extract_signup_otp(
        mail_body["text"]
            .as_str()
            .context("signup mail text missing")?,
    )?;

    let confirmed = router
        .clone()
        .oneshot(json_request(
            "/auth/signup/confirm",
            json!({"email": email, "otp": otp}),
        )?)
        .await
        .context("send synthetic signup confirm")?;
    let confirmed_body = bounded_json(confirmed, "signup confirm").await?;
    let confirmed_id = auth_account_id(&confirmed_body, "signup confirm")?;

    let replay = router
        .clone()
        .oneshot(json_request(
            "/auth/signup/confirm",
            json!({"email": email, "otp": otp}),
        )?)
        .await
        .context("send synthetic signup replay")?;
    anyhow::ensure!(
        replay.status() == StatusCode::NOT_FOUND,
        "signup replay must be absent"
    );
    anyhow::ensure!(
        mail.received_requests()
            .await
            .context("read replay mail")?
            .len()
            == 1,
        "signup replay must not send mail"
    );

    let login = router
        .oneshot(json_request(
            "/auth/login",
            json!({"email": email, "password": SYNTHETIC_PASSWORD}),
        )?)
        .await
        .context("send synthetic login")?;
    let login_body = bounded_json(login, "signup login").await?;
    let login_id = auth_account_id(&login_body, "signup login")?;
    anyhow::ensure!(login_id == confirmed_id, "signup/login account mismatch");
    let access_token = login_body["access_token"]
        .as_str()
        .filter(|token| !token.is_empty())
        .context("synthetic login access token missing")?
        .to_string();

    let account = crate::db::accounts::Account::fetch_by_email(pool, email)?
        .context("synthetic candidate missing after confirm")?;
    anyhow::ensure!(
        account.id == confirmed_id
            && account.email == SYNTHETIC_CANDIDATE
            && account.email_verified_at.is_some()
            && !account.is_admin
            && !account.is_temporary,
        "synthetic candidate account state mismatch"
    );
    Ok(SignedUpCandidate {
        account_id: login_id,
        access_token,
    })
}

fn json_request(path: &str, body: Value) -> Result<Request<Body>> {
    Ok(Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body)?))?)
}

async fn bounded_json(response: axum::response::Response, stage: &str) -> Result<Value> {
    anyhow::ensure!(
        response.status() == StatusCode::OK,
        "{stage} status mismatch"
    );
    serde_json::from_slice(&to_bytes(response.into_body(), MAX_AUTH_BODY_BYTES).await?)
        .with_context(|| format!("decode {stage} response"))
}

fn auth_account_id(body: &Value, stage: &str) -> Result<String> {
    let account = body
        .get("account")
        .with_context(|| format!("{stage} account missing"))?;
    let id = account
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        .with_context(|| format!("{stage} account id missing"))?;
    anyhow::ensure!(
        account.get("email").and_then(Value::as_str) == Some(SYNTHETIC_CANDIDATE)
            && account.get("is_admin").and_then(Value::as_bool) == Some(false)
            && account.get("is_temporary").and_then(Value::as_bool) == Some(false),
        "{stage} account state mismatch"
    );
    Ok(id.to_string())
}

fn extract_signup_otp(text: &str) -> Result<String> {
    const PREFIX: &str = "Your verification code is:\n\n";
    let suffix = text
        .strip_prefix("Welcome to Bluey.\n\n")
        .context("signup mail prefix")?;
    let code_and_tail = suffix.strip_prefix(PREFIX).context("signup otp prefix")?;
    let code = code_and_tail
        .get(..6)
        .filter(|value| value.bytes().all(|byte| byte.is_ascii_digit()))
        .context("signup otp absent")?;
    anyhow::ensure!(
        code_and_tail.as_bytes().get(6) == Some(&b'\n'),
        "signup otp framing"
    );
    Ok(code.to_string())
}
