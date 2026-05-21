//! Codex Stage 23: end-to-end integration tests with wiremock.
//!
//! Spins up the server's full Axum router pointed at wiremock instances
//! that stand in for OpenAI / Anthropic / Deepgram / Stripe. Exercises
//! the customer money-path top-to-bottom.

#![cfg(test)]


use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use serial_test::serial;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use bluey_server::config::{Config, UpstreamKeys};
use bluey_server::db::{open_pool, run_migrations, DbPool};

/// Test harness: starts wiremocks, builds an AppState pointed at them,
/// returns the axum Router ready for ServiceExt::oneshot.
#[allow(dead_code)]
struct Harness {
    pub router: axum::Router,
    pub pool: DbPool,
    pub openai: MockServer,
    pub anthropic: MockServer,
    pub stripe: MockServer,
    pub deepgram: MockServer,
}

async fn boot_harness() -> Harness {
    let openai = MockServer::start().await;
    let anthropic = MockServer::start().await;
    let stripe = MockServer::start().await;
    let deepgram = MockServer::start().await;

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
        smtp: None,
        upstream: UpstreamKeys {
            openai_api_key: Some("sk-test-openai".to_string()),
            anthropic_api_key: Some("sk-test-anthropic".to_string()),
            deepgram_api_key: Some("dg-test".to_string()),
            ollama_base_url: None,
        },
    };

    // Override upstream URLs by env. The dispatcher reads from
    // hard-coded URLs today; for the wiremock harness we need the
    // dispatcher to honor BLUEY_TEST_OPENAI_URL etc. That's a small
    // patch to dispatcher.rs covered in Stage 23 commit so this test
    // can hit the mock.
    std::env::set_var("BLUEY_TEST_OPENAI_URL", openai.uri());
    std::env::set_var("BLUEY_TEST_ANTHROPIC_URL", anthropic.uri());
    std::env::set_var("BLUEY_TEST_STRIPE_URL", stripe.uri());
    std::env::set_var("BLUEY_TEST_DEEPGRAM_URL", deepgram.uri());

    let router = bluey_server::api::build_router(pool.clone(), config);

    Harness {
        router,
        pool,
        openai,
        anthropic,
        stripe,
        deepgram,
    }
}

async fn signup_and_login(harness: &Harness, email: &str, password: &str) -> String {
    let req = Request::post("/auth/signup")
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
    assert_eq!(resp.status(), 200, "signup failed");
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    v["access_token"].as_str().unwrap().to_string()
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
