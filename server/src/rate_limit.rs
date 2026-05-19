#![allow(clippy::doc_lazy_continuation)]
//! Per-IP rate limiting for sensitive endpoints.
//!
//! Codex Stage 11: brute-force protection on /auth/login + /auth/signup
//! + /auth/refresh + /auth/device/poll. Uses the governor crate
//! per-key keyed rate limiter with an in-memory state map.
//!
//! Limits (per IP):
//!
//! - /auth/login + /auth/signup: 5 per minute, burst 5
//! - /auth/refresh: 30 per minute, burst 30
//! - /auth/device/poll: 60 per minute, long-poll friendly
//! - /router/complete: 120 per minute, tier-aware in v0.2.x
//!
//! Enforcement is best-effort: behind a load balancer the IP we see
//! is the LB's, so v0.2.x will need to honor X-Forwarded-For when
//! behind Caddy/Cloudflare.

use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::collections::HashMap;
use tokio::sync::Mutex;

type KeyedLimiterMap =
    Arc<Mutex<HashMap<String, Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>>;

/// One quota tier for a class of endpoint.
#[derive(Clone)]
pub struct Limiter {
    inner: KeyedLimiterMap,
    quota: Quota,
}

impl Limiter {
    pub fn new(per_minute: u32, burst: u32) -> Self {
        // governor::Quota::per_minute returns a quota of N/min.
        // burst is the maximum bucket capacity.
        let quota = Quota::per_minute(NonZeroU32::new(per_minute.max(1)).unwrap())
            .allow_burst(NonZeroU32::new(burst.max(1)).unwrap());
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            quota,
        }
    }

    /// Acquire a token for `key` (typically the client IP). Returns
    /// Ok(()) on allow, Err(retry_after_secs) on deny.
    pub async fn check(&self, key: &str) -> Result<(), u64> {
        let mut map = self.inner.lock().await;
        let limiter = map
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(RateLimiter::direct(self.quota)))
            .clone();
        drop(map);
        match limiter.check() {
            Ok(_) => Ok(()),
            Err(neg) => {
                // governor 0.6: NotUntil carries the delay until next allowed
                // request. Use earliest_possible() relative to now for a
                // simple seconds-to-retry estimate.
                use governor::clock::Clock;
                let clock = DefaultClock::default();
                let retry = neg.wait_time_from(clock.now());
                Err(retry.as_secs().max(1))
            }
        }
    }
}

/// Bundle of all per-endpoint limiters. Stored on AppState.
#[derive(Clone)]
pub struct RateLimiters {
    pub auth_login: Limiter,
    pub auth_signup: Limiter,
    pub auth_refresh: Limiter,
    pub auth_device_poll: Limiter,
    pub router_complete: Limiter,
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self {
            auth_login: Limiter::new(5, 5),
            auth_signup: Limiter::new(5, 5),
            auth_refresh: Limiter::new(30, 30),
            auth_device_poll: Limiter::new(60, 60),
            router_complete: Limiter::new(120, 60),
        }
    }
}

/// Extract a stable client identifier. Order:
///   1. X-Forwarded-For (when behind a trusted reverse proxy).
///   2. SocketAddr from ConnectInfo.
fn client_key(req: &Request<Body>) -> String {
    if let Some(xff) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = xff.to_str() {
            // First IP in the chain is the customer.
            if let Some(first) = s.split(',').next() {
                return first.trim().to_string();
            }
        }
    }
    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }
    "unknown".to_string()
}

macro_rules! make_middleware {
    ($name:ident, $field:ident) => {
        pub async fn $name(
            State(state): State<crate::api::AppState>,
            req: Request<Body>,
            next: Next,
        ) -> Result<Response, (StatusCode, [(axum::http::HeaderName, String); 1])> {
            let key = client_key(&req);
            match state.rate_limiters.$field.check(&key).await {
                Ok(()) => Ok(next.run(req).await),
                Err(retry) => Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    [(axum::http::header::RETRY_AFTER, retry.to_string())],
                )),
            }
        }
    };
}

make_middleware!(limit_auth_login, auth_login);
make_middleware!(limit_auth_signup, auth_signup);
make_middleware!(limit_auth_refresh, auth_refresh);
make_middleware!(limit_auth_device_poll, auth_device_poll);
make_middleware!(limit_router_complete, router_complete);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn limiter_allows_within_burst() {
        let l = Limiter::new(60, 5);
        for _ in 0..5 {
            assert!(l.check("ip1").await.is_ok());
        }
    }

    #[tokio::test]
    async fn limiter_denies_after_burst_exhausted() {
        let l = Limiter::new(60, 3);
        for _ in 0..3 {
            l.check("ip2").await.unwrap();
        }
        let res = l.check("ip2").await;
        assert!(res.is_err());
        let retry = res.unwrap_err();
        assert!(retry >= 1);
    }

    #[tokio::test]
    async fn limiter_keys_are_isolated() {
        let l = Limiter::new(60, 2);
        l.check("ip3").await.unwrap();
        l.check("ip3").await.unwrap();
        // ip3 exhausted; ip4 still has its own bucket.
        assert!(l.check("ip4").await.is_ok());
    }
}
