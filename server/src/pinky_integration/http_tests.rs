use super::*;
use axum::{body::Body, http::Request};
use jsonwebtoken::{encode, EncodingKey, Header};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const SECRET: &str = "synthetic-pinky-delegation-secret-at-least-32-bytes";

fn delegation_config() -> DelegationConfig {
    DelegationConfig::new(
        SECRET.into(),
        "pinky-preprod".into(),
        "bluey-assist".into(),
        "preprod".into(),
    )
    .unwrap()
}

fn fixture() -> (DbPool, std::path::PathBuf, crate::config::Config) {
    use crate::config::{Config, ServerDbBackend, TrialAbuseConfig, UpstreamKeys};
    let path = std::env::temp_dir().join(format!("pinky-http-{}.db", uuid::Uuid::new_v4()));
    let pool = crate::db::open_pool(&path).unwrap();
    crate::db::run_migrations(&pool).unwrap();
    let config = Config {
        port: 0,
        db_path: path.clone(),
        db_backend: ServerDbBackend::Sqlite,
        database_url: None,
        jwt_secret: "synthetic-standalone-secret-independent-of-pinky".into(),
        public_url: "http://localhost".into(),
        stripe_secret_key: None,
        stripe_webhook_secret: None,
        smtp: None,
        upstream: UpstreamKeys::default(),
        upstream_spend_guard: None,
        admin_emails: vec![],
        trial_abuse: TrialAbuseConfig::default(),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: false,
        object_storage: None,
        log_storage: None,
    };
    (pool, path, config)
}

fn token(subject: &str, path: &str, body: &[u8]) -> String {
    let now = Utc::now().timestamp();
    let claims = delegation::DelegationClaims {
        iss: "pinky-preprod".into(),
        aud: "bluey-assist".into(),
        env: "preprod".into(),
        sub: subject.into(),
        scope: "ai:session".into(),
        iat: now,
        exp: now + 60,
        jti: uuid::Uuid::new_v4().to_string(),
        method: "POST".into(),
        path: path.into(),
        body_sha256: hex::encode(Sha256::digest(body)),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

async fn request(app: &Router, subject: &str, path: &str, body: &[u8]) -> StatusCode {
    app.clone()
        .oneshot(
            Request::post(path)
                .header(
                    "Authorization",
                    format!("Bearer {}", token(subject, path, body)),
                )
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn session_http_boundary_fences_owner_body_and_route() {
    let (pool, path, _) = fixture();
    let app = routes(pool.clone(), delegation_config()).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let body = serde_json::to_vec(&serde_json::json!({"session_id":id})).unwrap();
    let open = "/integrations/pinky/sessions";
    let close = "/integrations/pinky/sessions/close";
    assert_eq!(request(&app, "owner-a", open, &body).await, StatusCode::OK);
    assert_eq!(
        request(&app, "owner-b", close, &body).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(request(&app, "owner-a", close, &body).await, StatusCode::OK);
    assert_eq!(request(&app, "owner-a", close, &body).await, StatusCode::OK);
    let tampered = app
        .clone()
        .oneshot(
            Request::post(open)
                .header(
                    "Authorization",
                    format!("Bearer {}", token("owner-a", open, &body)),
                )
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tampered.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        request(&app, "owner-a", "/router/complete/stream", &body).await,
        StatusCode::NOT_FOUND
    );
    let query = app
        .clone()
        .oneshot(
            Request::post(format!("{open}?extra=1"))
                .header(
                    "Authorization",
                    format!("Bearer {}", token("owner-a", open, &body)),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(query.status(), StatusCode::UNAUTHORIZED);
    drop(app);
    drop(pool);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn merged_router_is_additive_and_default_off_without_schema_mutation() {
    let (pool, path, config) = fixture();
    let off = build_application(pool.clone(), config.clone(), None).unwrap();
    let count: i64 = pool
        .get()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name LIKE 'pinky_ai_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let body = serde_json::to_vec(&serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string()
    }))
    .unwrap();
    assert_eq!(
        request(&off, "a", "/integrations/pinky/sessions", &body).await,
        StatusCode::NOT_FOUND
    );

    let account = crate::db::accounts::Account::create_with_admin_and_trial_seconds(
        &pool,
        "synthetic@example.invalid",
        "!test",
        false,
        0,
    )
    .unwrap();
    let access = crate::auth::jwt::issue(
        &config.jwt_secret,
        &account.id,
        crate::auth::jwt::TokenKind::Access,
    )
    .unwrap();
    let on = build_application(pool.clone(), config, Some(delegation_config())).unwrap();
    for app in [&off, &on] {
        let health = app
            .clone()
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        let me = app
            .clone()
            .oneshot(
                Request::get("/account/me")
                    .header("Authorization", format!("Bearer {access}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me.status(), StatusCode::OK);
    }
    let delegated = on
        .clone()
        .oneshot(
            Request::get("/account/me")
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        token("a", "/integrations/pinky/sessions", &body)
                    ),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delegated.status(), StatusCode::UNAUTHORIZED);
    let standalone = on
        .clone()
        .oneshot(
            Request::post("/integrations/pinky/sessions")
                .header("Authorization", format!("Bearer {access}"))
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(standalone.status(), StatusCode::UNAUTHORIZED);
    let started = on
        .clone()
        .oneshot(
            Request::post("/integrations/pinky/sessions")
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        token("a", "/integrations/pinky/sessions", &body)
                    ),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::OK);
    assert!(started
        .headers()
        .contains_key(cue_core::BLUEY_REQUEST_ID_HEADER));
    assert!(started
        .headers()
        .contains_key(cue_core::BLUEY_TRACE_ID_HEADER));
    drop(on);
    drop(off);
    drop(pool);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn admission_is_bounded_and_start_exhaustion_preserves_stop_budget() {
    let (pool, path, _) = fixture();
    let app = routes(pool.clone(), delegation_config()).unwrap();
    for _ in 0..40 {
        let response = app
            .clone()
            .oneshot(
                Request::post("/integrations/pinky/sessions")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let body = serde_json::to_vec(&serde_json::json!({
        "session_id":uuid::Uuid::new_v4().to_string()
    }))
    .unwrap();
    for _ in 0..30 {
        assert_eq!(
            request(&app, "a", "/integrations/pinky/sessions", &body).await,
            StatusCode::OK
        );
    }
    let limited = app
        .clone()
        .oneshot(
            Request::post("/integrations/pinky/sessions")
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        token("a", "/integrations/pinky/sessions", &body)
                    ),
                )
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(limited
        .headers()
        .contains_key(axum::http::header::RETRY_AFTER));
    assert!(limited
        .headers()
        .contains_key(cue_core::BLUEY_REQUEST_ID_HEADER));
    assert_eq!(
        request(&app, "a", "/integrations/pinky/sessions/close", &body).await,
        StatusCode::OK
    );
    drop(app);
    drop(pool);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn body_bounds_and_database_outages_have_sanitized_statuses() {
    let (pool, path, _) = fixture();
    let app = routes(pool.clone(), delegation_config()).unwrap();
    let oversized = app
        .clone()
        .oneshot(
            Request::post("/integrations/pinky/sessions")
                .body(Body::from(vec![b'x'; 16 * 1024 + 1]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    pool.get()
        .unwrap()
        .execute("DROP TABLE pinky_ai_sessions", [])
        .unwrap();
    let body = serde_json::to_vec(&serde_json::json!({
        "session_id": uuid::Uuid::new_v4().to_string()
    }))
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post("/integrations/pinky/sessions")
                .header(
                    "Authorization",
                    format!(
                        "Bearer {}",
                        token("a", "/integrations/pinky/sessions", &body)
                    ),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        serde_json::json!({"error":"session_unavailable"})
    );
    drop(app);
    drop(pool);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn configuration_is_default_off_and_requires_independent_complete_credentials() {
    use std::env::VarError;
    for flag in [None, Some("0")] {
        let config = config_from_lookup("standalone", |name| {
            assert_eq!(name, "BLUEY_PINKY_INTEGRATION_ENABLED");
            flag.map(str::to_owned).ok_or(VarError::NotPresent)
        })
        .unwrap();
        assert!(config.is_none());
    }
    assert!(config_from_lookup("standalone", |_| Ok("invalid".into())).is_err());
    assert!(config_from_lookup("standalone", |name| {
        if name == "BLUEY_PINKY_INTEGRATION_ENABLED" {
            Ok("1".into())
        } else {
            Err(VarError::NotPresent)
        }
    })
    .is_err());
    let lookup = |name: &str| {
        Ok(match name {
            "BLUEY_PINKY_INTEGRATION_ENABLED" => "1",
            "BLUEY_PINKY_DELEGATION_SECRET" => SECRET,
            "BLUEY_PINKY_ISSUER" => "pinky-preprod",
            "BLUEY_PINKY_AUDIENCE" => "bluey-assist",
            "BLUEY_PINKY_ENVIRONMENT" => "preprod",
            _ => panic!("unexpected configuration key"),
        }
        .to_owned())
    };
    assert!(config_from_lookup(SECRET, lookup).is_err());
    assert!(config_from_lookup("standalone", lookup).unwrap().is_some());
}
