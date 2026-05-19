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
    let claims = jwt::verify(&state.config.jwt_secret, &token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    if claims.kind != "access" {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let account = Account::fetch_by_id(&state.pool, &claims.sub)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    req.extensions_mut().insert(AuthedAccount(account));
    Ok(next.run(req).await)
}

fn extract_bearer(req: &Request<Body>) -> Option<String> {
    let raw = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ").map(|s| s.trim().to_string())
}
