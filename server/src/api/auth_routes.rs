//! Auth endpoints — real implementations.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::{auth, db::accounts::Account};

// ─── Request / response shapes ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub account: AuthAccountSummary,
}

#[derive(Serialize)]
pub struct AuthAccountSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: msg.to_string(),
        }),
    )
}

fn auth_response(
    state: &AppState,
    account: &Account,
) -> Result<AuthResponse, (StatusCode, Json<ApiError>)> {
    let access = auth::jwt::issue(
        &state.config.jwt_secret,
        &account.id,
        auth::jwt::TokenKind::Access,
    )
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("issue access: {e}"),
        )
    })?;
    let refresh = auth::jwt::issue(
        &state.config.jwt_secret,
        &account.id,
        auth::jwt::TokenKind::Refresh,
    )
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("issue refresh: {e}"),
        )
    })?;

    auth::refresh_store::store(&state.pool, &refresh, &account.id, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("store refresh: {e}"),
        )
    })?;

    Ok(AuthResponse {
        access_token: access,
        refresh_token: refresh,
        expires_in: auth::jwt::ACCESS_TTL_SECS,
        account: AuthAccountSummary {
            id: account.id.clone(),
            email: account.email.clone(),
            balance_cents: account.balance_cents,
            trial_seconds_remaining: account.trial_seconds_remaining,
        },
    })
}

// ─── Endpoint handlers ──────────────────────────────────────────────────

pub async fn signup(
    State(state): State<AppState>,
    Json(req): Json<SignupRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "email is required and must contain @",
        ));
    }

    // Reject duplicate email up-front (DB UNIQUE catches it too, but the
    // user-facing error is friendlier).
    if Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .is_some()
    {
        return Err(err(StatusCode::CONFLICT, "email already registered"));
    }

    let password_hash = auth::password::hash_password(&req.password)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e.to_string()))?;

    let account = Account::create(&state.pool, &email, &password_hash).map_err(|e| {
        // Codex Stage 10 round-2 typed-error nit: downcast to the typed
        // AccountCreateError::DuplicateEmail variant instead of
        // string-matching the error message. Race-free 409 mapping.
        if matches!(
            e.downcast_ref::<crate::db::accounts::AccountCreateError>(),
            Some(crate::db::accounts::AccountCreateError::DuplicateEmail)
        ) {
            return err(StatusCode::CONFLICT, "email already registered");
        }
        err(StatusCode::INTERNAL_SERVER_ERROR, &format!("create: {e}"))
    })?;

    Ok(Json(auth_response(&state, &account)?))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    let email = req.email.trim().to_lowercase();

    let stored_hash = match Account::password_hash(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
    {
        Some(h) => h,
        None => return Err(err(StatusCode::UNAUTHORIZED, "invalid credentials")),
    };

    if !auth::password::verify_password(&req.password, &stored_hash) {
        return Err(err(StatusCode::UNAUTHORIZED, "invalid credentials"));
    }

    let account = Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "account vanished after auth",
            )
        })?;

    Ok(Json(auth_response(&state, &account)?))
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    // 1. Verify the JWT signature + expiry.
    let claims = auth::jwt::verify(&state.config.jwt_secret, &req.refresh_token)
        .map_err(|_| err(StatusCode::UNAUTHORIZED, "invalid refresh token"))?;
    if claims.kind != "refresh" {
        return Err(err(StatusCode::UNAUTHORIZED, "not a refresh token"));
    }

    // 2. ATOMICALLY consume the refresh token: revoke if and only if we are
    // the unique caller to claim it. Race-free against concurrent /auth/refresh
    // calls. (Codex S2.3 blocker fix.)
    let account_id = auth::refresh_store::consume(&state.pool, &req.refresh_token)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "refresh token revoked or expired"))?;

    if account_id != claims.sub {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "refresh token / claim mismatch",
        ));
    }

    let account = Account::fetch_by_id(&state.pool, &account_id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "account not found"))?;

    // 3. Issue a new pair (the old token is already revoked atomically above).
    Ok(Json(auth_response(&state, &account)?))
}

// ─── Device flow ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct DeviceStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Deserialize)]
pub struct DevicePollRequest {
    pub device_code: String,
}

#[derive(Deserialize)]
pub struct DeviceApproveRequest {
    pub user_code: String,
}

const DEVICE_CODE_TTL_SECS: i64 = 600; // 10 minutes
const DEVICE_POLL_INTERVAL_SECS: i64 = 5;

fn random_user_code() -> String {
    // 8-character base32-style code, formatted as XXXX-XXXX. Avoids
    // visually ambiguous chars (no 0/O, no 1/I/l).
    use getrandom::getrandom;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut bytes = [0u8; 8];
    getrandom(&mut bytes).expect("OS random source");
    let mut out = String::with_capacity(9);
    for (i, b) in bytes.iter().enumerate() {
        if i == 4 {
            out.push('-');
        }
        out.push(ALPHABET[(*b as usize) % ALPHABET.len()] as char);
    }
    out
}

fn random_device_code() -> String {
    use getrandom::getrandom;
    let mut bytes = [0u8; 32];
    getrandom(&mut bytes).expect("OS random source");
    hex::encode(bytes)
}

pub async fn device_start(
    State(state): State<AppState>,
) -> Result<Json<DeviceStartResponse>, (StatusCode, Json<ApiError>)> {
    let device_code = random_device_code();
    let user_code = random_user_code();
    let expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(DEVICE_CODE_TTL_SECS)).to_rfc3339();
    let conn = state
        .pool
        .get()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    conn.execute(
        "INSERT INTO device_codes (device_code, user_code, expires_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![&device_code, &user_code, &expires_at],
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    Ok(Json(DeviceStartResponse {
        device_code,
        user_code,
        verification_uri: format!("{}/device", state.config.public_url),
        expires_in: DEVICE_CODE_TTL_SECS,
        interval: DEVICE_POLL_INTERVAL_SECS,
    }))
}

pub async fn device_poll(
    State(state): State<AppState>,
    Json(req): Json<DevicePollRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    let conn = state
        .pool
        .get()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    let row: Option<(Option<String>, i64, String)> = conn
        .query_row(
            "SELECT account_id, approved, expires_at FROM device_codes WHERE device_code = ?1",
            rusqlite::params![&req.device_code],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .ok();

    let Some((account_id_opt, approved, expires_at)) = row else {
        return Err(err(StatusCode::NOT_FOUND, "unknown device_code"));
    };
    let exp: chrono::DateTime<chrono::Utc> = expires_at
        .parse()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "bad expires_at"))?;
    if exp < chrono::Utc::now() {
        return Err(err(StatusCode::GONE, "device_code expired"));
    }
    if approved == 0 {
        return Err(err(StatusCode::ACCEPTED, "authorization_pending"));
    }
    let Some(account_id) = account_id_opt else {
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "approved but no account",
        ));
    };

    // Fetch account and issue tokens. Delete the device_code row to
    // make it single-use.
    let account = Account::fetch_by_id(&state.pool, &account_id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "account not found"))?;
    let _ = conn.execute(
        "DELETE FROM device_codes WHERE device_code = ?1",
        rusqlite::params![&req.device_code],
    );
    Ok(Json(auth_response(&state, &account)?))
}

pub async fn device_approve(
    State(state): State<AppState>,
    axum::Extension(crate::auth::AuthedAccount(account)): axum::Extension<
        crate::auth::AuthedAccount,
    >,
    Json(req): Json<DeviceApproveRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let conn = state
        .pool
        .get()
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    // Bind the device_code to the *currently authenticated* customer's account.
    // The auth middleware has already verified the JWT and loaded the account;
    // we use that account_id to mark the device approved.
    let now = chrono::Utc::now().to_rfc3339();
    let n = conn
        .execute(
            "UPDATE device_codes SET approved = 1, account_id = ?1
             WHERE user_code = ?2 AND expires_at > ?3",
            rusqlite::params![&account.id, &req.user_code, &now],
        )
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    if n == 0 {
        return Err(err(StatusCode::NOT_FOUND, "unknown or expired user_code"));
    }
    Ok(StatusCode::OK)
}

// ─── Codex Stage 13: email verification + password reset ────────────────

#[derive(Deserialize)]
pub struct ConfirmEmailVerify {
    pub token: String,
}

pub async fn verify_email_start(
    State(state): State<AppState>,
    Extension(crate::auth::AuthedAccount(account)): Extension<crate::auth::AuthedAccount>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, Json<ApiError>)> {
    use crate::db::auth_tokens;
    let tok = auth_tokens::mint(
        &state.pool,
        &account.id,
        auth_tokens::TokenKind::EmailVerification,
    )
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("mint: {e}"),
            }),
        )
    })?;
    let verify_url = format!("{}/verify-email?token={tok}", state.config.public_url);
    match crate::mail::send_email_verification(&state.config, &account.email, &verify_url).await {
        Ok(crate::mail::MailDelivery::Sent) => {
            tracing::info!(account_id = %account.id, email = %account.email, "email verification sent");
        }
        Ok(crate::mail::MailDelivery::NotConfigured) => {
            tracing::info!(
                account_id = %account.id,
                email = %account.email,
                verify_url = %verify_url,
                "email verification (SMTP unconfigured; logging dev URL)"
            );
        }
        Err(error) => {
            tracing::warn!(account_id = %account.id, email = %account.email, %error, "email verification delivery failed");
            return Err(err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "email delivery failed",
            ));
        }
    }
    Ok(axum::http::StatusCode::ACCEPTED)
}

pub async fn verify_email_confirm(
    State(state): State<AppState>,
    Json(req): Json<ConfirmEmailVerify>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, Json<ApiError>)> {
    use crate::db::auth_tokens;
    let account_id = auth_tokens::consume(
        &state.pool,
        &req.token,
        auth_tokens::TokenKind::EmailVerification,
    )
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("consume: {e}"),
            }),
        )
    })?
    .ok_or((
        axum::http::StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "invalid or expired verification token".into(),
        }),
    ))?;
    let conn = state.pool.get().map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("pool: {e}"),
            }),
        )
    })?;
    let _ = conn.execute(
        "UPDATE accounts SET email_verified_at = datetime('now') WHERE id = ?1",
        rusqlite::params![&account_id],
    );
    Ok(axum::http::StatusCode::OK)
}

#[derive(Deserialize)]
pub struct StartPasswordReset {
    pub email: String,
}

#[derive(Deserialize)]
pub struct ConfirmPasswordReset {
    pub token: String,
    pub new_password: String,
}

pub async fn password_reset_start(
    State(state): State<AppState>,
    Json(req): Json<StartPasswordReset>,
) -> axum::http::StatusCode {
    use crate::db::accounts::Account;
    use crate::db::auth_tokens;
    // Always return 202 (don't leak whether the email exists).
    let email = req.email.trim().to_lowercase();
    if let Ok(Some(account)) = Account::fetch_by_email(&state.pool, &email) {
        if let Ok(tok) = auth_tokens::mint(
            &state.pool,
            &account.id,
            auth_tokens::TokenKind::PasswordReset,
        ) {
            let reset_url = format!("{}/password-reset?token={tok}", state.config.public_url);
            match crate::mail::send_password_reset(&state.config, &account.email, &reset_url).await
            {
                Ok(crate::mail::MailDelivery::Sent) => {
                    tracing::info!(account_id = %account.id, email = %account.email, "password reset sent");
                }
                Ok(crate::mail::MailDelivery::NotConfigured) => {
                    tracing::info!(
                        account_id = %account.id,
                        email = %account.email,
                        reset_url = %reset_url,
                        "password reset (SMTP unconfigured; logging dev URL)"
                    );
                }
                Err(error) => {
                    tracing::warn!(account_id = %account.id, email = %account.email, %error, "password reset delivery failed");
                }
            }
        }
    }
    axum::http::StatusCode::ACCEPTED
}

pub async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(req): Json<ConfirmPasswordReset>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, Json<ApiError>)> {
    use crate::auth::password;
    use crate::db::auth_tokens;
    // Codex S12-17 nit: validate + hash BEFORE consuming the token, so
    // a 400 (password too short / too long / etc) does not burn the
    // single-use reset and force the customer to start over.
    let new_hash = password::hash_password(&req.new_password).map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: e.to_string(),
            }),
        )
    })?;
    let account_id = auth_tokens::consume(
        &state.pool,
        &req.token,
        auth_tokens::TokenKind::PasswordReset,
    )
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("consume: {e}"),
            }),
        )
    })?
    .ok_or((
        axum::http::StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "invalid or expired reset token".into(),
        }),
    ))?;
    let conn = state.pool.get().map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("pool: {e}"),
            }),
        )
    })?;
    let _ = conn.execute(
        "UPDATE accounts SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![&new_hash, &account_id],
    );
    Ok(axum::http::StatusCode::OK)
}

// ─── Codex Stage 18: deep-link handoff (Onboarding Option A) ────────────

#[derive(serde::Serialize)]
pub struct LinkMintResponse {
    /// The raw one-time code. Browser includes this in the bluey://
    /// deep link redirect.
    pub link_code: String,
    /// Pre-built deep link URL the browser should redirect to.
    pub deep_link_url: String,
    /// Validity in seconds (always 300 today; client may show a
    /// countdown).
    pub expires_in_secs: i64,
}

pub async fn link_mint(
    State(state): State<AppState>,
    Extension(crate::auth::AuthedAccount(account)): Extension<crate::auth::AuthedAccount>,
) -> Result<Json<LinkMintResponse>, (StatusCode, Json<ApiError>)> {
    use crate::auth::{jwt, refresh_store};
    use crate::db::link_codes;

    // Mint fresh tokens specifically for this device so the browser
    // session and the device do not share refresh tokens.
    let access = jwt::issue(
        &state.config.jwt_secret,
        &account.id,
        jwt::TokenKind::Access,
    )
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("issue access: {e}"),
        )
    })?;
    let refresh_raw = jwt::issue(
        &state.config.jwt_secret,
        &account.id,
        jwt::TokenKind::Refresh,
    )
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("issue refresh: {e}"),
        )
    })?;
    refresh_store::store(&state.pool, &refresh_raw, &account.id, Some("device-link")).map_err(
        |e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("store refresh: {e}"),
            )
        },
    )?;

    let code = link_codes::mint(&state.pool, &account.id, &access, &refresh_raw)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("mint: {e}")))?;

    let deep_link = format!("bluey://link?code={code}");
    Ok(Json(LinkMintResponse {
        link_code: code,
        deep_link_url: deep_link,
        expires_in_secs: 300,
    }))
}

#[derive(serde::Deserialize)]
pub struct LinkExchangeRequest {
    pub code: String,
}

#[derive(serde::Serialize)]
pub struct LinkExchangeResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub account: AuthAccountSummary,
}

pub async fn link_exchange(
    State(state): State<AppState>,
    Json(req): Json<LinkExchangeRequest>,
) -> Result<Json<LinkExchangeResponse>, (StatusCode, Json<ApiError>)> {
    use crate::db::accounts::Account;
    use crate::db::link_codes;

    let (account_id, access, refresh) = link_codes::exchange(&state.pool, &req.code)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("exchange: {e}")))?
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "invalid or expired link code"))?;

    let account = Account::fetch_by_id(&state.pool, &account_id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("fetch: {e}")))?
        .ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "account vanished"))?;

    Ok(Json(LinkExchangeResponse {
        access_token: access,
        refresh_token: refresh,
        account: AuthAccountSummary {
            id: account.id,
            email: account.email,
            balance_cents: account.balance_cents,
            trial_seconds_remaining: account.trial_seconds_remaining,
        },
    }))
}
