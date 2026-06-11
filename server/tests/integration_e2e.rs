//! Codex Stage 23: end-to-end integration tests with wiremock.
//!
//! Spins up the server's full Axum router pointed at wiremock instances
//! that stand in for OpenAI / Anthropic / Deepgram / Stripe. Exercises
//! the customer money-path top-to-bottom.

#![cfg(test)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use serial_test::serial;
use tower::ServiceExt;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use bluey_server::auth;
use bluey_server::config::{Config, SmtpConfig, UpstreamKeys, UpstreamSpendGuard};
use bluey_server::db::accounts::Account;
use bluey_server::db::usage::{self, UsageEvent};
use bluey_server::db::{idempotency, open_pool, run_migrations, DbPool};

/// Test harness: starts wiremocks, builds an AppState pointed at them,
/// returns the axum Router ready for ServiceExt::oneshot.
struct Harness {
    pub router: axum::Router,
    pub pool: DbPool,
    pub openai: MockServer,
    pub anthropic: MockServer,
    pub stripe: MockServer,
    pub square: MockServer,
    pub deepgram: MockServer,
    pub mail: MockServer,
}

async fn boot_harness() -> Harness {
    boot_harness_with_upstream(UpstreamKeys {
        openai_api_key: Some("sk-test-openai".to_string()),
        anthropic_api_key: Some("sk-test-anthropic".to_string()),
        deepgram_api_key: Some("dg-test".to_string()),
        ollama_base_url: None,
    })
    .await
}

async fn boot_harness_with_upstream(upstream: UpstreamKeys) -> Harness {
    boot_harness_with_upstream_and_admin_emails(upstream, vec![]).await
}

async fn boot_harness_with_upstream_and_admin_emails(
    upstream: UpstreamKeys,
    admin_emails: Vec<String>,
) -> Harness {
    boot_harness_with_options(upstream, admin_emails, None).await
}

async fn boot_harness_with_options(
    upstream: UpstreamKeys,
    admin_emails: Vec<String>,
    upstream_spend_guard: Option<UpstreamSpendGuard>,
) -> Harness {
    let openai = MockServer::start().await;
    let anthropic = MockServer::start().await;
    let stripe = MockServer::start().await;
    let square = MockServer::start().await;
    let deepgram = MockServer::start().await;
    let mail = MockServer::start().await;

    let path = std::env::temp_dir().join(format!("bluey-e2e-{}.db", uuid::Uuid::new_v4()));
    let pool = open_pool(&path).unwrap();
    run_migrations(&pool).unwrap();

    let config = Config {
        port: 0,
        db_path: path,
        jwt_secret: "test-secret-at-least-32-chars-long-xxx".to_string(),
        public_url: "http://localhost:8080".to_string(),
        stripe_secret_key: Some("sk_test_e2e".to_string()),
        stripe_webhook_secret: Some("whsec_test_e2e".to_string()),
        smtp: Some(SmtpConfig {
            host: "smtp.resend.com".to_string(),
            port: 587,
            username: Some("resend".to_string()),
            password: Some("test-resend-key".to_string()),
            from: "Bluey <hello@bluey.sh>".to_string(),
            starttls: true,
        }),
        upstream,
        upstream_spend_guard,
        admin_emails,
    };

    // Override upstream URLs by env. The dispatcher reads from
    // hard-coded URLs today; for the wiremock harness we need the
    // dispatcher to honor BLUEY_TEST_OPENAI_URL etc. That's a small
    // patch to dispatcher.rs covered in Stage 23 commit so this test
    // can hit the mock.
    std::env::set_var("BLUEY_TEST_OPENAI_URL", openai.uri());
    std::env::set_var("BLUEY_TEST_ANTHROPIC_URL", anthropic.uri());
    std::env::set_var("BLUEY_TEST_STRIPE_URL", stripe.uri());
    std::env::set_var("BLUEY_TEST_SQUARE_URL", square.uri());
    std::env::set_var("BLUEY_TEST_DEEPGRAM_URL", deepgram.uri());
    std::env::set_var("BLUEY_RESEND_API_BASE_URL", mail.uri());

    let router = bluey_server::api::build_router(pool.clone(), config);

    Harness {
        router,
        pool,
        openai,
        anthropic,
        stripe,
        square,
        deepgram,
        mail,
    }
}

fn sample_usage(request_id: &str, bluey_cost_cents: i64) -> UsageEvent {
    UsageEvent {
        request_id: request_id.to_string(),
        kind: "llm".to_string(),
        task_type: None,
        lane: Some("instant".to_string()),
        provider: Some("openai".to_string()),
        model: Some("gpt-5.4-mini".to_string()),
        input_tokens: 10,
        output_tokens: 5,
        latency_ms: 20,
        cost_cents_to_bluey: bluey_cost_cents,
        cost_cents_to_customer: bluey_cost_cents,
        was_speculative: false,
        was_fallback: false,
    }
}

async fn signup_and_login(harness: &Harness, email: &str, password: &str) -> String {
    let normalized_email = email.trim().to_lowercase();
    let password_hash = auth::password::hash_password(password).unwrap();
    Account::create(&harness.pool, &normalized_email, &password_hash).unwrap();
    let auth = login(harness, &normalized_email, password).await;
    auth["access_token"].as_str().unwrap().to_string()
}

async fn login(harness: &Harness, email: &str, password: &str) -> serde_json::Value {
    let req = Request::post("/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200, "login failed");
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn assert_admin_customers_allowed(harness: &Harness, access: &str) {
    let req = Request::get("/admin/customers")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = harness.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

fn extract_six_digit_code(text: &str) -> Option<String> {
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            run.push(ch);
            if run.len() == 6 {
                return Some(run);
            }
        } else {
            run.clear();
        }
    }
    None
}

async fn signup_with_otp(harness: &Harness, email: &str, password: &str) -> serde_json::Value {
    Mock::given(method("POST"))
        .and(path("/emails"))
        .and(header("Authorization", "Bearer test-resend-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":"email-otp"})))
        .mount(&harness.mail)
        .await;

    let start = Request::post("/auth/signup/start")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "password": password
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(start).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let requests = harness.mail.received_requests().await.unwrap();
    let mail_body: serde_json::Value =
        serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    assert_eq!(
        mail_body["to"][0].as_str().unwrap(),
        email.trim().to_lowercase()
    );
    assert_eq!(mail_body["subject"], "Your Bluey verification code");
    let code = extract_six_digit_code(mail_body["text"].as_str().unwrap()).unwrap();

    let confirm = Request::post("/auth/signup/confirm")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": email,
                "otp": code
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = harness.router.clone().oneshot(confirm).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
#[serial]
async fn signup_otp_email_confirms_and_marks_email_verified() {
    let h = boot_harness().await;

    let email = "otp-smoke@bluey.sh";
    let password = "longenoughpw";
    let auth = signup_with_otp(&h, email, password).await;
    assert_eq!(auth["account"]["email"], email);
    assert!(auth["access_token"].as_str().unwrap().len() > 20);

    let conn = h.pool.get().unwrap();
    let verified_at: Option<String> = conn
        .query_row(
            "SELECT email_verified_at FROM accounts WHERE email = ?1",
            rusqlite::params![email],
            |r| r.get(0),
        )
        .unwrap();
    assert!(verified_at.is_some());
}

#[tokio::test]
#[serial]
async fn configured_admin_email_signup_gets_admin_access() {
    let h = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec!["owner@bluey.sh".to_string()],
    )
    .await;

    let auth = signup_with_otp(&h, " Owner@Bluey.SH ", "longenoughpw").await;
    let access = auth["access_token"].as_str().unwrap();
    assert_admin_customers_allowed(&h, access).await;

    let req = Request::get("/account/me")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let me: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(me["email"], "owner@bluey.sh");
    assert_eq!(me["is_admin"], true);
}

#[tokio::test]
#[serial]
async fn legacy_signup_endpoint_is_retired() {
    let h = boot_harness().await;

    let req = Request::post("/auth/signup")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "email": "legacy-signup@bluey.sh",
                "password": "longenoughpw"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GONE);
}

#[tokio::test]
#[serial]
async fn configured_admin_email_login_promotes_existing_account() {
    let h = boot_harness_with_upstream_and_admin_emails(
        UpstreamKeys::default(),
        vec!["late-admin@bluey.sh".to_string()],
    )
    .await;

    let password_hash = auth::password::hash_password("longenoughpw").unwrap();
    let account = Account::create(&h.pool, "late-admin@bluey.sh", &password_hash).unwrap();
    assert!(!account.is_admin);

    let auth = login(&h, "late-admin@bluey.sh", "longenoughpw").await;
    assert_eq!(auth["account"]["is_admin"], true);
    let access = auth["access_token"].as_str().unwrap();
    assert_admin_customers_allowed(&h, access).await;
}

#[tokio::test]
#[serial]
async fn router_complete_happy_path_with_mocked_openai() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "happy@example.com", "longenoughpw").await;

    // Mock OpenAI Chat Completions.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "Hello back!"}}],
            "usage": {"prompt_tokens": 12, "completion_tokens": 4}
        })))
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "test-req-1",
                "system": "you are helpful",
                "user": "hello",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
#[serial]
async fn router_complete_upstream_spend_guard_blocks_before_provider_hit() {
    let h = boot_harness_with_options(
        UpstreamKeys {
            openai_api_key: Some("sk-test-openai".to_string()),
            anthropic_api_key: Some("sk-test-anthropic".to_string()),
            deepgram_api_key: Some("dg-test".to_string()),
            ollama_base_url: None,
        },
        vec![],
        Some(UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        }),
    )
    .await;
    let access = signup_and_login(&h, "budget@example.com", "longenoughpw").await;
    let account = Account::fetch_by_email(&h.pool, "budget@example.com")
        .unwrap()
        .unwrap();
    usage::record(&h.pool, &account.id, &sample_usage("spent", 10)).unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "should not happen"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .expect(0)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "budget-guard",
                "system": "you are helpful",
                "user": "hello",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["reason"], "upstream_spend_guard");
}

#[tokio::test]
#[serial]
async fn router_complete_idempotency_replay_returns_cached() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "idem@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "First call"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2}
        })))
        .expect(1) // critical: only ONE upstream hit even on retry.
        .mount(&h.openai)
        .await;

    for _ in 0..2 {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": "test-idem-1",
                    "system": "",
                    "user": "hi",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }
}

#[tokio::test]
#[serial]
async fn router_complete_stream_proxies_openai_deltas_then_billing() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stream-openai@example.com", "longenoughpw").await;

    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"stream\"}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":4}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-1",
                "system": "you are helpful",
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    let first_delta = body.find("Hello ").expect("missing first upstream delta");
    let second_delta = body.find("stream").expect("missing second upstream delta");
    let billing = body.find("event: billing").expect("missing billing event");
    assert!(first_delta < billing, "delta must arrive before billing");
    assert!(second_delta < billing, "delta must arrive before billing");
    assert!(body.contains("\"text\":\"Hello stream\""));
    assert!(body.contains("\"provider\":\"openai\""));
    assert!(body.contains("\"model\":\"gpt-5.4-mini\""));
    assert!(body.contains("\"input_tokens\":12"));
    assert!(body.contains("\"output_tokens\":4"));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
#[serial]
async fn router_complete_stream_openai_error_frame_is_retryable() {
    let h = boot_harness().await;
    let email = "stream-openai-error@example.com";
    let access = signup_and_login(&h, email, "longenoughpw").await;

    let stream = concat!(
        "data: {\"error\":{\"message\":\"provider overloaded\",\"type\":\"rate_limit_error\"}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-openai-error-1",
                "system": "you are helpful",
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("event: error"));
    assert!(body.contains("upstream_stream_error"));
    assert!(!body.contains("event: billing"));

    let account = Account::fetch_by_email(&h.pool, email).unwrap().unwrap();
    let replay = idempotency::reserve(&h.pool, &account.id, "stream-openai-error-1").unwrap();
    assert_eq!(replay, idempotency::ReserveOutcome::FreshReservation);
}

#[tokio::test]
#[serial]
async fn router_complete_stream_idempotency_replays_cached_stream_without_upstream() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stream-idem@example.com", "longenoughpw").await;

    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Cached \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"stream\"}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":3}}\n\n",
        "data: [DONE]\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    for _ in 0..2 {
        let req = Request::post("/router/complete/stream")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": "stream-idem-1",
                    "system": "you are helpful",
                    "user": "answer quickly",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Cached "));
        assert!(body.contains("stream"));
        assert!(body.contains("event: billing"));
        assert!(body.contains("\"text\":\"Cached stream\""));
        assert!(body.contains("data: [DONE]"));
    }
}

#[tokio::test]
#[serial]
async fn router_complete_stream_proxies_anthropic_messages_sse() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stream-anthropic@example.com", "longenoughpw").await;

    let stream = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":20,\"output_tokens\":0}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Deep \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"answer\"}}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":6}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .expect(1)
        .mount(&h.anthropic)
        .await;

    let req = Request::post("/router/complete/stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "stream-anthropic-1",
                "system": "you are helpful",
                "user": "answer a normal technical question",
                "lane": "balanced"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 128 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    let first_delta = body.find("Deep ").expect("missing first upstream delta");
    let second_delta = body.find("answer").expect("missing second upstream delta");
    let billing = body.find("event: billing").expect("missing billing event");
    assert!(first_delta < billing, "delta must arrive before billing");
    assert!(second_delta < billing, "delta must arrive before billing");
    assert!(body.contains("\"text\":\"Deep answer\""));
    assert!(body.contains("\"provider\":\"anthropic\""));
    assert!(body.contains("\"model\":\"claude-sonnet-4-6\""));
    assert!(body.contains("\"input_tokens\":20"));
    assert!(body.contains("\"output_tokens\":6"));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
#[serial]
async fn router_complete_falls_back_when_preferred_provider_429s() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "fallback@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .expect(1)
        .mount(&h.anthropic)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "fallback answer"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "fallback-429-1",
                "system": "you are helpful",
                "user": "answer a normal technical question",
                "lane": "balanced"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "fallback answer");
    assert_eq!(v["provider"], "openai");
    assert_eq!(v["model"], "gpt-5.4");
}

#[tokio::test]
#[serial]
async fn router_complete_retries_next_openai_key_on_429_without_customer_wait() {
    let upstream = UpstreamKeys {
        openai_api_key: Some("sk-openai-a,sk-openai-b".to_string()),
        anthropic_api_key: Some("sk-test-anthropic".to_string()),
        deepgram_api_key: Some("dg-test".to_string()),
        ollama_base_url: None,
    };
    let request_id = "openai-keypool-429-1";
    let ordered_keys =
        upstream.key_candidates("openai", &format!("llm:{request_id}:openai:gpt-5.4-mini"));
    assert_eq!(ordered_keys.len(), 2);

    let h = boot_harness_with_upstream(upstream).await;
    let access = signup_and_login(&h, "keypool@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            format!("Bearer {}", ordered_keys[0].secret),
        ))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "45")
                .set_body_string("rate limited"),
        )
        .expect(1)
        .mount(&h.openai)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            format!("Bearer {}", ordered_keys[1].secret),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "second key answered"}}],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": request_id,
                "system": "you are helpful",
                "user": "answer quickly",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "second key answered");
    assert_eq!(v["provider"], "openai");
    assert_eq!(v["model"], "gpt-5.4-mini");
}

#[tokio::test]
#[serial]
async fn router_complete_reports_upstream_error_after_capacity_skip() {
    std::env::set_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN", "60");
    std::env::set_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN_BURST", "1");
    let h = boot_harness().await;
    std::env::remove_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN_BURST");
    let access = signup_and_login(&h, "fallback-capacity@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [{"type": "text", "text": "first anthropic ok"}],
            "usage": {"input_tokens": 10, "output_tokens": 4}
        })))
        .expect(1)
        .mount(&h.anthropic)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("temporary openai failure"))
        .expect(1)
        .mount(&h.openai)
        .await;

    for (request_id, expected_status) in [("capacity-skip-1", 200), ("capacity-skip-2", 502)] {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": request_id,
                    "system": "you are helpful",
                    "user": "answer a normal technical question",
                    "lane": "balanced"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), expected_status);
        if expected_status == 502 {
            let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["error"], "upstream provider error; please retry");
            assert!(v.get("retry_after_secs").is_none());
        }
    }
}

#[tokio::test]
#[serial]
async fn router_complete_enforces_account_burst_before_second_upstream_hit() {
    std::env::set_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN", "60");
    std::env::set_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN_BURST", "1");
    let h = boot_harness().await;
    std::env::remove_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN");
    std::env::remove_var("BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN_BURST");

    let access = signup_and_login(&h, "burst@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "first ok"}}],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2}
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    for (idx, expected_status) in [(1, 200), (2, 429)] {
        let req = Request::post("/router/complete")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access}"))
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "request_id": format!("burst-limit-{idx}"),
                    "system": "",
                    "user": "hi",
                    "lane": "instant"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = h.router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), expected_status);
        if idx == 2 {
            let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(v["reason"], "account_llm_busy");
            assert!(v["retry_after_secs"].as_u64().unwrap_or(0) >= 1);
        }
    }
}

#[tokio::test]
#[serial]
async fn billing_checkout_uses_mocked_stripe() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stripe@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/checkout/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "cs_test_123",
            "url": "https://checkout.stripe.test/session/cs_test_123"
        })))
        .expect(1)
        .mount(&h.stripe)
        .await;

    let req = Request::post("/billing/checkout")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({ "amount_cents": 3000 })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v["checkout_url"],
        "https://checkout.stripe.test/session/cs_test_123"
    );
}

#[tokio::test]
#[serial]
async fn auth_link_mint_then_exchange_roundtrip() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "link@example.com", "longenoughpw").await;

    let req = Request::post("/auth/link/mint")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from("{}".as_bytes().to_vec()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let code = v["link_code"].as_str().unwrap().to_string();
    assert!(v["deep_link_url"]
        .as_str()
        .unwrap()
        .starts_with("bluey://link?code="));

    let req = Request::post("/auth/link/exchange")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"code": code})).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
#[serial]
async fn auth_device_poll_is_single_use_after_approval() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "device-once@example.com", "longenoughpw").await;

    let req = Request::post("/auth/device/start")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let started: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        started["verification_uri"].as_str().unwrap(),
        "http://localhost:8080/login"
    );
    let device_code = started["device_code"].as_str().unwrap();
    let user_code = started["user_code"].as_str().unwrap();

    let req = Request::post("/auth/device/approve")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "user_code": user_code })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let poll_body = serde_json::to_vec(&json!({ "device_code": device_code })).unwrap();
    let req = Request::post("/auth/device/poll")
        .header("content-type", "application/json")
        .body(Body::from(poll_body.clone()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::post("/auth/device/poll")
        .header("content-type", "application/json")
        .body(Body::from(poll_body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn auth_device_approve_cannot_overwrite_approved_code() {
    let h = boot_harness().await;
    let first_access = signup_and_login(&h, "device-owner@example.com", "longenoughpw").await;
    let second_access = signup_and_login(&h, "device-attacker@example.com", "longenoughpw").await;

    let req = Request::post("/auth/device/start")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let started: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let device_code = started["device_code"].as_str().unwrap();
    let user_code = started["user_code"].as_str().unwrap();

    let approve_body = serde_json::to_vec(&json!({ "user_code": user_code })).unwrap();
    let req = Request::post("/auth/device/approve")
        .header("authorization", format!("Bearer {first_access}"))
        .header("content-type", "application/json")
        .body(Body::from(approve_body.clone()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::post("/auth/device/approve")
        .header("authorization", format!("Bearer {second_access}"))
        .header("content-type", "application/json")
        .body(Body::from(approve_body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let req = Request::post("/auth/device/poll")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "device_code": device_code })).unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let auth: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(auth["account"]["email"], "device-owner@example.com");
}

#[tokio::test]
#[serial]
async fn sync_batch_session_bundle_and_rag_roundtrip() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "sync@example.com", "longenoughpw").await;

    let batch = json!({
        "sessions": [{
            "session_id": "sess-cloud-1",
            "title": "Cloud sync test",
            "status": "active",
            "created_at_ms": 1000,
            "updated_at_ms": 2000,
            "last_active_at_ms": 2000,
            "answer_style": "be concise",
            "metadata": {"source": "test"}
        }],
        "transcript_segments": [{
            "segment_id": "seg-cloud-1",
            "session_id": "sess-cloud-1",
            "speaker": "system",
            "source": "system",
            "text": "We discussed queue backpressure and cache stampede controls.",
            "ts_ms": 1500,
            "is_final": true
        }],
        "cue_responses": [{
            "response_id": "resp-cloud-1",
            "session_id": "sess-cloud-1",
            "kind": "answer",
            "text": "Use bounded queues, retries, and admission control.",
            "ts_ms": 1600,
            "provider": "bluey-managed-instant",
            "model": "gpt-5.4-mini",
            "cost_label": "$0.01 · balance $29.99"
        }],
        "context_artifacts": [{
            "artifact_id": "ctx-cloud-1",
            "session_id": "sess-cloud-1",
            "kind": "document",
            "title": "Architecture brief",
            "text_preview": "The architecture uses bounded queues.",
            "created_at_ms": 1400
        }],
        "rag_chunks": [{
            "chunk_id": "chunk-cloud-1",
            "session_id": "sess-cloud-1",
            "source_kind": "transcript",
            "source_id": "seg-cloud-1",
            "chunk_index": 0,
            "text": "queue backpressure cache stampede",
            "updated_at_ms": 1500
        }]
    });

    let req = Request::post("/sync/batch")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(serde_json::to_vec(&batch).unwrap()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let req = Request::get("/sync/sessions")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(listed["sessions"][0]["session_id"], "sess-cloud-1");

    let req = Request::get("/sync/sessions/sess-cloud-1")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let bundle: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        bundle["transcript_segments"][0]["segment_id"],
        "seg-cloud-1"
    );
    assert_eq!(
        bundle["cue_responses"][0]["cost_label"],
        "$0.01 · balance $29.99"
    );

    let req = Request::post("/rag/query")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "query": "cache stampede",
                "top_k": 3
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let rag: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(rag["matches"][0]["chunk_id"], "chunk-cloud-1");
}

#[tokio::test]
#[serial]
async fn router_transcribe_happy_path_with_mocked_deepgram() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stt@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/listen"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "metadata": { "duration": 1.4 },
            "results": {
                "channels": [
                    { "alternatives": [ { "transcript": "hello from deepgram" } ] }
                ]
            }
        })))
        .expect(1)
        .mount(&h.deepgram)
        .await;

    let req = Request::post("/router/transcribe?request_id=test-stt-1")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "audio/wav")
        .body(Body::from(vec![1u8; 32_000]))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "hello from deepgram");
    assert_eq!(v["provider"], "deepgram");
    assert_eq!(v["model"], "nova-3");
    assert_eq!(v["duration_seconds"], 2);
}

#[tokio::test]
#[serial]
async fn router_transcribe_falls_back_to_openai_when_deepgram_fails() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "stt-fallback@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v1/listen"))
        .respond_with(ResponseTemplate::new(503).set_body_string("deepgram busy"))
        .expect(1)
        .mount(&h.deepgram)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "text": "hello from openai fallback"
        })))
        .expect(1)
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/transcribe?request_id=test-stt-fallback-1")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "audio/wav")
        .body(Body::from(vec![1u8; 32_000]))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["text"], "hello from openai fallback");
    assert_eq!(v["provider"], "openai");
    assert_eq!(v["model"], "gpt-4o-mini-transcribe");
    assert_eq!(v["duration_seconds"], 2);
}

#[tokio::test]
#[serial]
async fn billing_portal_creates_session_via_mocked_stripe() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "portal@example.com", "longenoughpw").await;

    // Seed: customer must have stripe_customer_id (the portal endpoint
    // 400s if missing). We poke it directly into the DB to simulate a
    // prior successful Checkout.
    let conn = h.pool.get().unwrap();
    conn.execute(
        "UPDATE accounts SET stripe_customer_id = ?1 WHERE email = ?2",
        rusqlite::params!["cus_test_portal", "portal@example.com"],
    )
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/billing_portal/sessions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "bps_test",
            "url": "https://billing.stripe.com/p/session/test_session_url"
        })))
        .expect(1)
        .mount(&h.stripe)
        .await;

    let req = Request::post("/billing/portal")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["portal_url"]
        .as_str()
        .unwrap()
        .starts_with("https://billing.stripe.com/"));
}

#[tokio::test]
#[serial]
async fn billing_checkout_creates_square_payment_link_when_square_enabled() {
    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");

    let h = boot_harness().await;
    let access = signup_and_login(&h, "square-checkout@example.com", "longenoughpw").await;

    Mock::given(method("POST"))
        .and(path("/v2/online-checkout/payment-links"))
        .and(header("Square-Version", "2025-04-16"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "payment_link": {
                "id": "LNK_TEST",
                "url": "https://square.link/u/bluey-test"
            }
        })))
        .expect(1)
        .mount(&h.square)
        .await;

    let req = Request::post("/billing/checkout")
        .header("authorization", format!("Bearer {access}"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "amount_cents": 3000
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["checkout_url"], "https://square.link/u/bluey-test");

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
}

#[tokio::test]
#[serial]
async fn billing_square_webhook_credits_completed_order() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "square-whsec");

    let h = boot_harness().await;
    let _access = signup_and_login(&h, "square-webhook@example.com", "longenoughpw").await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_credit_1",
        "type": "order.updated",
        "data": {
            "object": {
                "order": {
                    "id": "order_1",
                    "state": "COMPLETED",
                    "reference_id": format!("bluey_reload:{account_id}"),
                    "metadata": {
                        "bluey_account_id": account_id,
                        "bluey_amount_cents": "3000"
                    },
                    "total_money": {"amount": 3000, "currency": "USD"},
                    "tenders": [{"payment_id": "payment_square_1"}]
                }
            }
        }
    }))
    .unwrap();

    let url = "http://localhost:8080/billing/square/webhook";
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"square-whsec").unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents FROM accounts WHERE email = ?1",
            rusqlite::params!["square-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(balance, 3000);

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn billing_square_webhook_accepts_production_signature_while_checkout_is_sandbox() {
    use base64::Engine;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let url = "http://localhost:8080/billing/square/webhook";
    std::env::set_var("BLUEY_BILLING_PROVIDER", "square");
    std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
    std::env::set_var("SQUARE_WEBHOOK_NOTIFICATION_URL", url);
    std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
    std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
    std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "sandbox-square-whsec");
    std::env::set_var("SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY", "prod-square-whsec");

    let h = boot_harness().await;
    let _access = signup_and_login(
        &h,
        "square-prod-webhook@example.com",
        "longenoughpw",
    )
    .await;
    let account_id: String = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT id FROM accounts WHERE email = ?1",
            rusqlite::params!["square-prod-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();

    let body = serde_json::to_string(&json!({
        "event_id": "evt_square_prod_credit_1",
        "type": "order.updated",
        "data": {
            "object": {
                "order": {
                    "id": "order_prod_1",
                    "state": "COMPLETED",
                    "reference_id": format!("bluey_reload:{account_id}"),
                    "metadata": {
                        "bluey_account_id": account_id,
                        "bluey_amount_cents": "1500"
                    },
                    "total_money": {"amount": 1500, "currency": "USD"},
                    "tenders": [{"payment_id": "payment_square_prod_1"}]
                }
            }
        }
    }))
    .unwrap();

    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"prod-square-whsec").unwrap();
    mac.update(url.as_bytes());
    mac.update(body.as_bytes());
    let signature = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    let req = Request::post("/billing/square/webhook")
        .header("x-square-hmacsha256-signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let balance: i64 = h
        .pool
        .get()
        .unwrap()
        .query_row(
            "SELECT balance_cents FROM accounts WHERE email = ?1",
            rusqlite::params!["square-prod-webhook@example.com"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(balance, 1500);

    std::env::remove_var("BLUEY_BILLING_PROVIDER");
    std::env::remove_var("SQUARE_ENVIRONMENT");
    std::env::remove_var("SQUARE_WEBHOOK_NOTIFICATION_URL");
    std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
    std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
    std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
    std::env::remove_var("SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY");
}

#[tokio::test]
#[serial]
async fn billing_portal_400s_without_stripe_customer() {
    let h = boot_harness().await;
    let access = signup_and_login(&h, "no-cus@example.com", "longenoughpw").await;

    let req = Request::post("/billing/portal")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::empty())
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
#[serial]
async fn auto_topup_off_by_default_does_not_fire_charge() {
    // New accounts default to auto_topup_enabled=1 in schema, but with
    // no PaymentMethod on file the topup helper short-circuits. We
    // exercise that path: low balance + no PM -> NO charge.
    let h = boot_harness().await;
    let access = signup_and_login(&h, "topup@example.com", "longenoughpw").await;

    // Drain trial seconds + balance to force the post-deduct branch to
    // be reached, but no Stripe customer/PM means topup is skipped.
    let conn = h.pool.get().unwrap();
    conn.execute(
        "UPDATE accounts SET trial_seconds_remaining = 0, balance_cents = 100 WHERE email = ?1",
        rusqlite::params!["topup@example.com"],
    )
    .unwrap();

    // No Stripe mock for /v1/payment_intents — if topup fired, it would
    // hit a non-existent endpoint and the test would still pass because
    // the spawn is fire-and-forget. But we assert the wiremock has
    // received ZERO matching POSTs to /v1/payment_intents.
    Mock::given(method("POST"))
        .and(path("/v1/payment_intents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "pi_should_not_fire",
            "status": "succeeded"
        })))
        .expect(0) // strict: must NOT be called.
        .mount(&h.stripe)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        })))
        .mount(&h.openai)
        .await;

    let req = Request::post("/router/complete")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {access}"))
        .body(Body::from(
            serde_json::to_vec(&json!({
                "request_id": "topup-test-no-pm",
                "system": "",
                "user": "hi",
                "lane": "instant"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.router.clone().oneshot(req).await.unwrap();
    // Either 200 (cue completed despite low balance via trial absorb) or
    // 402 (insufficient balance). Both are acceptable — the assertion is
    // that the Stripe payment_intents mock was NOT called.
    assert!(resp.status() == 200 || resp.status() == 402);

    // Give the spawned topup task a chance to run if it would.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
}
