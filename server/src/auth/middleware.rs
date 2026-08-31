//! Auth middleware. Extracts `Authorization: Bearer <jwt>`, verifies
//! the access token, fetches the account, attaches it as a request
//! extension for downstream handlers.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::{api::AppState, auth::jwt, db::accounts::Account};

/// Extension marker attached to authenticated requests.
#[derive(Clone)]
pub struct AuthedAccount(pub Account);

/// Axum middleware function. Wraps protected routes.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer(&req).ok_or(StatusCode::UNAUTHORIZED)?;
    let claims =
        jwt::verify(&state.config.jwt_secret, &token).map_err(|_| StatusCode::UNAUTHORIZED)?;
    if claims.kind != "access" {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let account = Account::fetch_by_id(&state.pool, &claims.sub)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if account.is_temporary_expired() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let deletion_pending =
        crate::db::account_data::account_deletion_is_pending(&state.pool, &account.id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deletion_pending && req.uri().path() != "/account/delete" {
        return Err(StatusCode::GONE);
    }
    req.extensions_mut().insert(AuthedAccount(account));
    Ok(next.run(req).await)
}

fn extract_bearer(req: &Request<Body>) -> Option<String> {
    let raw = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ").map(|s| s.trim().to_string())
}

/// Axum middleware function for admin-only routes. Must be layered
/// AFTER `require_auth` (which inserts the `AuthedAccount` extension).
/// Returns 401 if the auth extension is missing (misconfiguration or
/// effectively unauthenticated), 403 if the account is not admin.
pub async fn require_admin(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let authed = req
        .extensions()
        .get::<AuthedAccount>()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if !authed.0.is_admin {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod admin_tests {
    use super::*;
    use crate::db::accounts::Account;
    use axum::{middleware::from_fn, response::IntoResponse, routing::get, Extension, Router};
    use tower::ServiceExt;

    fn make_test_account(is_admin: bool) -> Account {
        Account {
            id: format!("test-id-{}", if is_admin { "admin" } else { "user" }),
            email: if is_admin {
                "admin@example.com".to_string()
            } else {
                "user@example.com".to_string()
            },
            email_verified_at: None,
            balance_cents: 0,
            trial_seconds_remaining: 0,
            is_temporary: false,
            temporary_expires_at: None,
            auto_topup_enabled: false,
            auto_topup_threshold_cents: 0,
            auto_topup_amount_cents: 0,
            is_admin,
            stripe_customer_id: None,
            stripe_payment_method_id: None,
            square_customer_id: None,
            square_card_id: None,
            square_card_brand: None,
            square_card_last4: None,
            billing_restricted: false,
            billing_restriction_reason: None,
            billing_restricted_at: None,
        }
    }

    async fn ok_handler() -> impl IntoResponse {
        (StatusCode::OK, "admin ok")
    }

    fn admin_app(authed: Option<Account>) -> Router {
        let mut app = Router::new()
            .route("/admin/test", get(ok_handler))
            .route_layer(from_fn(require_admin));
        if let Some(account) = authed {
            app = app.layer(Extension(AuthedAccount(account)));
        }
        app
    }

    #[tokio::test]
    async fn admin_route_rejects_unauthenticated() {
        let app = admin_app(None);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_route_rejects_non_admin() {
        let app = admin_app(Some(make_test_account(false)));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn admin_route_allows_admin() {
        let app = admin_app(Some(make_test_account(true)));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
