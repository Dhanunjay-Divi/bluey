//! Codex S12-17 blocker 2: prove ConnectInfo<SocketAddr> reaches the
//! rate-limit middleware in the REAL `axum::serve` path, not just under
//! `tower::ServiceExt::oneshot` which bypasses ConnectInfo install.

#![cfg(test)]

use std::net::SocketAddr;

use bluey_server::api::build_router;
use bluey_server::config::{Config, UpstreamKeys};
use bluey_server::db::accounts::Account;
use bluey_server::db::{open_pool, run_migrations};

#[tokio::test]
async fn real_serve_path_installs_connect_info_and_rate_limit_sees_peer_ip() {
    // 1. Build state + router exactly as production would.
    let path = std::env::temp_dir().join(format!("bluey-connectinfo-{}.db", uuid::Uuid::new_v4()));
    let pool = open_pool(&path).unwrap();
    run_migrations(&pool).unwrap();

    let admin_email = "admin-ci@example.com";
    let admin = Account::create(&pool, admin_email, "stub_hash").unwrap();
    // Promote to admin (the bare insert defaults is_admin=0).
    pool.get()
        .unwrap()
        .execute(
            "UPDATE accounts SET is_admin = 1 WHERE id = ?1",
            rusqlite::params![&admin.id],
        )
        .unwrap();

    let config = Config {
        port: 0,
        db_path: path,
        db_backend: bluey_server::config::ServerDbBackend::Sqlite,
        database_url: None,
        jwt_secret: "test-secret-at-least-32-chars-long-xxx".into(),
        public_url: "http://localhost".into(),
        stripe_secret_key: None,
        stripe_webhook_secret: None,
        smtp: None,
        upstream: UpstreamKeys::default(),
        upstream_spend_guard: None,
        admin_emails: vec![],
        trial_abuse: bluey_server::config::TrialAbuseConfig::default(),
        turnstile_site_key: None,
        turnstile_secret_key: None,
        require_turnstile: false,
        object_storage: None,
        log_storage: None,
    };

    let app = build_router(pool.clone(), config.clone());

    // 2. Bind to a random port and serve, exactly as production does.
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let local_addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // 3. Mint an admin access token so we can hit /admin/echo-peer.
    let access = bluey_server::auth::jwt::issue(
        &config.jwt_secret,
        &admin.id,
        bluey_server::auth::jwt::TokenKind::Access,
    )
    .unwrap();

    // 4. Hit /admin/echo-peer with a real reqwest client.
    let url = format!("http://{}/admin/echo-peer", local_addr);
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&access)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // 5. The peer key MUST be the loopback IP, NOT "unknown". This proves
    //    `into_make_service_with_connect_info::<SocketAddr>()` is wired.
    let peer = resp["peer_key"].as_str().unwrap_or("");
    assert!(
        peer == "127.0.0.1" || peer.starts_with("127."),
        "expected loopback peer IP, got '{}' — ConnectInfo is NOT installed",
        peer
    );
    assert_ne!(
        peer, "unknown",
        "rate-limit fell back to 'unknown' — ConnectInfo not installed in real serve path"
    );

    server.abort();
}
