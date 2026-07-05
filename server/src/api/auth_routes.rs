//! Auth endpoints — real implementations.

use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, StatusCode},
    Extension, Json,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::net::{IpAddr, SocketAddr};

use super::AppState;
use crate::{
    auth,
    db::{accounts::Account, device_codes, refresh_tokens, signup_otps, trial_abuse},
};

// ─── Request / response shapes ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SignupStartRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub turnstile_token: Option<String>,
    #[serde(default)]
    pub device_fingerprint: Option<String>,
}

#[derive(Serialize)]
pub struct SignupStartResponse {
    pub email: String,
    pub expires_in_secs: i64,
}

#[derive(Deserialize)]
pub struct SignupConfirmRequest {
    pub email: String,
    pub otp: String,
    #[serde(default)]
    pub device_fingerprint: Option<String>,
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

#[derive(Deserialize)]
pub struct LogoutRequest {
    pub refresh_token: Option<String>,
    pub revoke_all: Option<bool>,
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
    pub is_temporary: bool,
    pub temporary_expires_at: Option<String>,
    pub is_admin: bool,
}

#[derive(Deserialize)]
pub struct TrialStartRequest {
    #[serde(default)]
    pub turnstile_token: Option<String>,
    #[serde(default)]
    pub device_fingerprint: Option<String>,
}

#[derive(Serialize)]
pub struct TrialStartResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub account: AuthAccountSummary,
    pub password: String,
    pub trial_seconds: i64,
    pub temporary_expires_at: String,
}

#[derive(Deserialize)]
pub struct TrialConvertStartRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub terms_accepted: bool,
}

#[derive(Serialize)]
pub struct TrialConvertStartResponse {
    pub email: String,
    pub expires_in_secs: i64,
}

#[derive(Deserialize)]
pub struct TrialConvertConfirmRequest {
    pub email: String,
    pub otp: String,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Serialize)]
pub struct CaptchaConfigResponse {
    pub provider: Option<&'static str>,
    pub site_key: Option<String>,
}

#[derive(Deserialize)]
struct TurnstileVerifyResponse {
    success: bool,
    #[serde(default, rename = "error-codes")]
    error_codes: Vec<String>,
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: msg.to_string(),
        }),
    )
}

fn allow_dev_auth_link_logs() -> bool {
    matches!(
        std::env::var("BLUEY_DEV_LOG_AUTH_LINKS")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

const SIGNUP_OTP_TTL_SECS: i64 = 10 * 60;
const SIGNUP_OTP_MAX_ATTEMPTS: i64 = 5;
const TEMPORARY_TRIAL_SECONDS: i64 = 15 * 60;
const TEMPORARY_ACCOUNT_TTL_SECS: i64 = 24 * 60 * 60;
const TEMPORARY_ACCOUNT_EMAIL_DOMAIN: &str = "try.bluey.sh";

fn normalize_signup_email(email: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let email = email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "email is required and must contain @",
        ));
    }
    Ok(email)
}

fn random_signup_otp() -> String {
    use getrandom::getrandom;
    const OTP_SPACE: u32 = 1_000_000;
    let unbiased_zone = u32::MAX - (u32::MAX % OTP_SPACE);
    loop {
        let mut bytes = [0u8; 4];
        getrandom(&mut bytes).expect("OS random source");
        let value = u32::from_le_bytes(bytes);
        if value < unbiased_zone {
            return format!("{:06}", value % OTP_SPACE);
        }
    }
}

fn random_human_secret(len: usize) -> String {
    use getrandom::getrandom;
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut bytes = [0u8; 24];
        getrandom(&mut bytes).expect("OS random source");
        for byte in bytes {
            if out.len() == len {
                break;
            }
            out.push(ALPHABET[(byte as usize) % ALPHABET.len()] as char);
        }
    }
    out
}

fn temporary_trial_email() -> String {
    format!(
        "trial-{}@{}",
        random_human_secret(12).to_ascii_lowercase(),
        TEMPORARY_ACCOUNT_EMAIL_DOMAIN
    )
}

fn is_temporary_trial_email(email: &str) -> bool {
    let email = email.trim().to_ascii_lowercase();
    email.starts_with("trial-") && email.ends_with(&format!("@{TEMPORARY_ACCOUNT_EMAIL_DOMAIN}"))
}

fn signup_otp_hash(jwt_secret: &str, email: &str, otp: &str) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(jwt_secret.as_bytes())
        .expect("HMAC accepts arbitrary-length keys");
    mac.update(b"bluey-signup-otp-v1");
    mac.update(email.as_bytes());
    mac.update(otp.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in a.bytes().zip(b.bytes()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn header_first(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .find(|part| !part.is_empty())
                    .map(str::to_string)
            })
    })
}

fn request_ip(headers: &HeaderMap, peer_ip: Option<IpAddr>) -> Option<String> {
    crate::rate_limit::trusted_client_ip_from_headers(peer_ip, headers)
}

fn request_user_agent(headers: &HeaderMap) -> Option<String> {
    header_first(headers, &["user-agent"])
}

fn request_device_fingerprint(headers: &HeaderMap, body_value: Option<&str>) -> Option<String> {
    body_value
        .and_then(|value| {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        })
        .or_else(|| {
            header_first(
                headers,
                &["x-bluey-device-id", "x-bluey-device-fingerprint"],
            )
        })
}

fn request_trial_identity(
    headers: &HeaderMap,
    peer_ip: Option<IpAddr>,
    device_fingerprint: Option<&str>,
) -> trial_abuse::TrialAbuseSignals {
    let device = request_device_fingerprint(headers, device_fingerprint);
    let identity = device
        .as_deref()
        .map(|value| format!("try-us-device:{value}"))
        .or_else(|| {
            request_ip(headers, peer_ip).map(|ip| {
                format!(
                    "try-us-ip:{ip}:{}",
                    request_user_agent(headers).unwrap_or_default()
                )
            })
        })
        .unwrap_or_else(|| format!("try-us-random:{}", uuid::Uuid::new_v4()));
    trial_abuse::TrialAbuseSignals::from_raw(
        &identity,
        request_ip(headers, peer_ip).as_deref(),
        device.as_deref(),
        request_user_agent(headers).as_deref(),
    )
}

fn signup_signals(
    email: &str,
    headers: &HeaderMap,
    peer_ip: Option<IpAddr>,
    device_fingerprint: Option<&str>,
) -> trial_abuse::TrialAbuseSignals {
    trial_abuse::TrialAbuseSignals::from_raw(
        email,
        request_ip(headers, peer_ip).as_deref(),
        request_device_fingerprint(headers, device_fingerprint).as_deref(),
        request_user_agent(headers).as_deref(),
    )
}

fn bearer_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn account_still_usable(account: &Account) -> bool {
    !account.is_temporary_expired()
}

async fn verify_turnstile_if_needed(
    state: &AppState,
    headers: &HeaderMap,
    peer_ip: Option<IpAddr>,
    token: Option<&str>,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let Some(secret) = state.config.turnstile_secret_key.as_deref() else {
        if state.config.require_turnstile {
            return Err(err(
                StatusCode::SERVICE_UNAVAILABLE,
                "captcha verification is required but not configured",
            ));
        }
        return Ok(());
    };
    if state.config.require_turnstile && state.config.turnstile_site_key.is_none() {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "captcha verification is required but the site key is not configured",
        ));
    }
    let token = token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            err(
                StatusCode::FORBIDDEN,
                "captcha verification is required to create an account",
            )
        })?;

    let mut form = vec![
        ("secret".to_string(), secret.to_string()),
        ("response".to_string(), token.to_string()),
    ];
    if let Some(ip) = request_ip(headers, peer_ip) {
        form.push(("remoteip".to_string(), ip));
    }
    let response = reqwest::Client::new()
        .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
        .form(&form)
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(%error, "turnstile verification request failed");
            err(StatusCode::BAD_GATEWAY, "captcha verification unavailable")
        })?;
    let status = response.status();
    let parsed = response
        .json::<TurnstileVerifyResponse>()
        .await
        .map_err(|error| {
            tracing::warn!(%status, %error, "turnstile verification response was invalid");
            err(StatusCode::BAD_GATEWAY, "captcha verification unavailable")
        })?;
    if parsed.success {
        Ok(())
    } else {
        tracing::warn!(errors = ?parsed.error_codes, "turnstile rejected signup");
        Err(err(StatusCode::FORBIDDEN, "captcha verification failed"))
    }
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

    refresh_tokens::store(&state.pool, &refresh, &account.id, None).map_err(|e| {
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
            is_temporary: account.is_temporary,
            temporary_expires_at: account.temporary_expires_at.clone(),
            is_admin: account.is_admin,
        },
    })
}

// ─── Endpoint handlers ──────────────────────────────────────────────────

pub async fn captcha_config(State(state): State<AppState>) -> Json<CaptchaConfigResponse> {
    let site_key = state.config.turnstile_site_key.clone();
    Json(CaptchaConfigResponse {
        provider: site_key.as_ref().map(|_| "turnstile"),
        site_key,
    })
}

pub async fn trial_start(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(req): Json<TrialStartRequest>,
) -> Result<(StatusCode, Json<TrialStartResponse>), (StatusCode, Json<ApiError>)> {
    if let Some(token) = bearer_from_headers(&headers) {
        if let Ok(claims) = auth::jwt::verify(&state.config.jwt_secret, &token) {
            if claims.kind == "access" {
                if let Ok(Some(account)) = Account::fetch_by_id(&state.pool, &claims.sub) {
                    if account_still_usable(&account) {
                        return Err(err(
                            StatusCode::CONFLICT,
                            "already signed in; use the current account or sign out first",
                        ));
                    }
                }
            }
        }
    }

    let peer_ip = connect_info.map(|ConnectInfo(addr)| addr.ip());
    let signals = request_trial_identity(&headers, peer_ip, req.device_fingerprint.as_deref());
    if let Err(error) =
        verify_turnstile_if_needed(&state, &headers, peer_ip, req.turnstile_token.as_deref()).await
    {
        let _ = trial_abuse::record_event(
            &state.pool,
            None,
            &signals,
            "trial_turnstile_failed",
            2,
            Some("captcha_failed"),
        );
        return Err(error);
    }

    let decision =
        trial_abuse::evaluate_trial_grant(&state.pool, state.config.trial_abuse, &signals)
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("trial abuse: {e}"),
                )
            })?;
    if !decision.allowed {
        return Err(err(
            StatusCode::TOO_MANY_REQUESTS,
            decision.reason.as_deref().unwrap_or("trial limit reached"),
        ));
    }

    let password = random_human_secret(18);
    let password_hash = auth::password::hash_password(&password)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("password: {e}")))?;
    let temporary_expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(TEMPORARY_ACCOUNT_TTL_SECS)).to_rfc3339();
    let mut last_error: Option<anyhow::Error> = None;
    let mut account_result = None;
    for _ in 0..3 {
        let email = temporary_trial_email();
        match Account::create_temporary(
            &state.pool,
            &email,
            &password_hash,
            TEMPORARY_TRIAL_SECONDS,
            &temporary_expires_at,
        ) {
            Ok(account) => {
                account_result = Some(account);
                break;
            }
            Err(error)
                if matches!(
                    error.downcast_ref::<crate::db::accounts::AccountCreateError>(),
                    Some(crate::db::accounts::AccountCreateError::DuplicateEmail)
                ) => {}
            Err(error) => {
                last_error = Some(error);
                break;
            }
        }
    }
    let account = account_result.ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!(
                "create trial: {}",
                last_error
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "could not allocate temporary email".to_string())
            ),
        )
    })?;

    if let Err(error) =
        trial_abuse::record_grant(&state.pool, &account.id, &signals, TEMPORARY_TRIAL_SECONDS)
    {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            %error,
            "failed to record temporary trial grant"
        );
    }

    let auth = auth_response(&state, &account)?;
    Ok((
        StatusCode::CREATED,
        Json(TrialStartResponse {
            access_token: auth.access_token,
            refresh_token: auth.refresh_token,
            expires_in: auth.expires_in,
            account: auth.account,
            password,
            trial_seconds: TEMPORARY_TRIAL_SECONDS,
            temporary_expires_at,
        }),
    ))
}

pub async fn trial_convert_start(
    State(state): State<AppState>,
    Extension(crate::auth::AuthedAccount(account)): Extension<crate::auth::AuthedAccount>,
    Json(req): Json<TrialConvertStartRequest>,
) -> Result<Json<TrialConvertStartResponse>, (StatusCode, Json<ApiError>)> {
    if !account.is_temporary || account.is_temporary_expired() {
        return Err(err(
            StatusCode::CONFLICT,
            "this account is already saved or the temporary trial expired",
        ));
    }
    if !req.terms_accepted {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "accept Terms and Privacy to create an account",
        ));
    }
    let email = normalize_signup_email(&req.email)?;
    if is_temporary_trial_email(&email) {
        return Err(err(StatusCode::BAD_REQUEST, "enter a real email address"));
    }
    if Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .is_some()
    {
        return Err(err(StatusCode::CONFLICT, "email already registered"));
    }

    let password_hash = auth::password::hash_password(&req.password)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e.to_string()))?;
    let otp = random_signup_otp();
    let otp_hash = signup_otp_hash(&state.config.jwt_secret, &email, &otp);
    let expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(SIGNUP_OTP_TTL_SECS)).to_rfc3339();

    signup_otps::upsert_for_account(
        &state.pool,
        &email,
        &otp_hash,
        &password_hash,
        &expires_at,
        &account.id,
    )
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;

    match crate::mail::send_signup_otp(&state.config, &email, &otp, SIGNUP_OTP_TTL_SECS / 60).await
    {
        Ok(crate::mail::MailDelivery::Sent) => {}
        Ok(crate::mail::MailDelivery::NotConfigured) if allow_dev_auth_link_logs() => {
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                email_hash = %cue_core::account_id_hash_prefix(&email),
                signup_otp = %otp,
                "trial conversion OTP generated but SMTP is unconfigured; dev logging enabled"
            );
        }
        Ok(crate::mail::MailDelivery::NotConfigured) => {
            return Err(err(
                StatusCode::SERVICE_UNAVAILABLE,
                "email delivery unavailable",
            ));
        }
        Err(error) => {
            tracing::warn!(account_id_hash = %cue_core::account_id_hash_prefix(&account.id), %error, "trial conversion OTP delivery failed");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "email delivery failed",
            ));
        }
    }

    Ok(Json(TrialConvertStartResponse {
        email,
        expires_in_secs: SIGNUP_OTP_TTL_SECS,
    }))
}

pub async fn trial_convert_confirm(
    State(state): State<AppState>,
    Extension(crate::auth::AuthedAccount(account)): Extension<crate::auth::AuthedAccount>,
    Json(req): Json<TrialConvertConfirmRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    if !account.is_temporary || account.is_temporary_expired() {
        return Err(err(
            StatusCode::CONFLICT,
            "this account is already saved or the temporary trial expired",
        ));
    }
    let email = normalize_signup_email(&req.email)?;
    let otp = req.otp.trim();
    if otp.len() != 6 || !otp.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "verification code must be 6 digits",
        ));
    }
    let signup_otp = signup_otps::fetch(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "verification code not requested"))?;
    if signup_otp.account_id.as_deref() != Some(account.id.as_str()) {
        return Err(err(StatusCode::UNAUTHORIZED, "verification code mismatch"));
    }
    let exp: chrono::DateTime<chrono::Utc> = signup_otp
        .expires_at
        .parse()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "bad expires_at"))?;
    if exp < chrono::Utc::now() {
        let _ = signup_otps::delete(&state.pool, &email);
        return Err(err(StatusCode::GONE, "verification code expired"));
    }
    if signup_otp.attempts >= SIGNUP_OTP_MAX_ATTEMPTS {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "too many attempts"));
    }
    let submitted_hash = signup_otp_hash(&state.config.jwt_secret, &email, otp);
    if !constant_time_eq(&submitted_hash, &signup_otp.otp_hash) {
        let _ = signup_otps::increment_attempts(&state.pool, &email);
        return Err(err(StatusCode::UNAUTHORIZED, "invalid verification code"));
    }
    if Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .is_some()
    {
        return Err(err(StatusCode::CONFLICT, "email already registered"));
    }

    let converted = Account::convert_temporary_to_registered(
        &state.pool,
        &account.id,
        &email,
        &signup_otp.password_hash,
    )
    .map_err(|e| {
        if matches!(
            e.downcast_ref::<crate::db::accounts::AccountCreateError>(),
            Some(crate::db::accounts::AccountCreateError::DuplicateEmail)
        ) {
            return err(StatusCode::CONFLICT, "email already registered");
        }
        err(StatusCode::INTERNAL_SERVER_ERROR, &format!("create: {e}"))
    })?
    .ok_or_else(|| err(StatusCode::CONFLICT, "trial account is no longer temporary"))?;

    let _ = signup_otps::delete(&state.pool, &email);
    Ok(Json(auth_response(&state, &converted)?))
}

pub async fn signup_start(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(req): Json<SignupStartRequest>,
) -> Result<Json<SignupStartResponse>, (StatusCode, Json<ApiError>)> {
    let email = normalize_signup_email(&req.email)?;
    let peer_ip = connect_info.map(|ConnectInfo(addr)| addr.ip());

    if Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .is_some()
    {
        return Err(err(StatusCode::CONFLICT, "email already registered"));
    }

    let signals = signup_signals(&email, &headers, peer_ip, req.device_fingerprint.as_deref());
    if let Err(error) =
        verify_turnstile_if_needed(&state, &headers, peer_ip, req.turnstile_token.as_deref()).await
    {
        let _ = trial_abuse::record_event(
            &state.pool,
            None,
            &signals,
            "turnstile_failed",
            2,
            Some("captcha_failed"),
        );
        return Err(error);
    }

    let decision =
        trial_abuse::evaluate_trial_grant(&state.pool, state.config.trial_abuse, &signals)
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("trial abuse: {e}"),
                )
            })?;
    if !decision.allowed {
        return Err(err(
            StatusCode::TOO_MANY_REQUESTS,
            decision.reason.as_deref().unwrap_or("trial limit reached"),
        ));
    }

    let password_hash = auth::password::hash_password(&req.password)
        .map_err(|e| err(StatusCode::BAD_REQUEST, &e.to_string()))?;
    let otp = random_signup_otp();
    let otp_hash = signup_otp_hash(&state.config.jwt_secret, &email, &otp);
    let expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(SIGNUP_OTP_TTL_SECS)).to_rfc3339();

    signup_otps::upsert(&state.pool, &email, &otp_hash, &password_hash, &expires_at)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;

    match crate::mail::send_signup_otp(&state.config, &email, &otp, SIGNUP_OTP_TTL_SECS / 60).await
    {
        Ok(crate::mail::MailDelivery::Sent) => {
            tracing::info!(email_hash = %cue_core::account_id_hash_prefix(&email), "signup OTP sent");
        }
        Ok(crate::mail::MailDelivery::NotConfigured) if allow_dev_auth_link_logs() => {
            tracing::info!(
                email_hash = %cue_core::account_id_hash_prefix(&email),
                signup_otp = %otp,
                "signup OTP generated but SMTP is unconfigured; dev logging enabled"
            );
        }
        Ok(crate::mail::MailDelivery::NotConfigured) => {
            tracing::warn!(
                email_hash = %cue_core::account_id_hash_prefix(&email),
                "signup OTP generated but SMTP is unconfigured"
            );
            return Err(err(
                StatusCode::SERVICE_UNAVAILABLE,
                "email delivery unavailable",
            ));
        }
        Err(error) => {
            tracing::warn!(email_hash = %cue_core::account_id_hash_prefix(&email), %error, "signup OTP delivery failed");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "email delivery failed",
            ));
        }
    }

    Ok(Json(SignupStartResponse {
        email,
        expires_in_secs: SIGNUP_OTP_TTL_SECS,
    }))
}

pub async fn signup_confirm(
    State(state): State<AppState>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    headers: HeaderMap,
    Json(req): Json<SignupConfirmRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    let email = normalize_signup_email(&req.email)?;
    let peer_ip = connect_info.map(|ConnectInfo(addr)| addr.ip());
    let otp = req.otp.trim();
    if otp.len() != 6 || !otp.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "verification code must be 6 digits",
        ));
    }

    let signup_otp = signup_otps::fetch(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "verification code not requested"))?;
    if signup_otp.account_id.is_some() {
        return Err(err(
            StatusCode::CONFLICT,
            "verification code belongs to a temporary account conversion",
        ));
    }

    let exp: chrono::DateTime<chrono::Utc> = signup_otp
        .expires_at
        .parse()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "bad expires_at"))?;
    if exp < chrono::Utc::now() {
        let _ = signup_otps::delete(&state.pool, &email);
        return Err(err(StatusCode::GONE, "verification code expired"));
    }

    if signup_otp.attempts >= SIGNUP_OTP_MAX_ATTEMPTS {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "too many attempts"));
    }

    let submitted_hash = signup_otp_hash(&state.config.jwt_secret, &email, otp);
    if !constant_time_eq(&submitted_hash, &signup_otp.otp_hash) {
        let _ = signup_otps::increment_attempts(&state.pool, &email);
        return Err(err(StatusCode::UNAUTHORIZED, "invalid verification code"));
    }

    if Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .is_some()
    {
        return Err(err(StatusCode::CONFLICT, "email already registered"));
    }

    let signals = signup_signals(&email, &headers, peer_ip, req.device_fingerprint.as_deref());
    let decision =
        trial_abuse::evaluate_trial_grant(&state.pool, state.config.trial_abuse, &signals)
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("trial abuse: {e}"),
                )
            })?;
    if !decision.allowed {
        return Err(err(
            StatusCode::TOO_MANY_REQUESTS,
            decision.reason.as_deref().unwrap_or("trial limit reached"),
        ));
    }

    let is_admin = state.config.is_admin_email(&email);
    let account =
        Account::create_with_admin(&state.pool, &email, &signup_otp.password_hash, is_admin)
            .map_err(|e| {
                if matches!(
                    e.downcast_ref::<crate::db::accounts::AccountCreateError>(),
                    Some(crate::db::accounts::AccountCreateError::DuplicateEmail)
                ) {
                    return err(StatusCode::CONFLICT, "email already registered");
                }
                err(StatusCode::INTERNAL_SERVER_ERROR, &format!("create: {e}"))
            })?;

    let _ = signup_otps::delete(&state.pool, &email);
    Account::mark_email_verified(&state.pool, &account.id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    if let Err(error) = trial_abuse::record_grant(
        &state.pool,
        &account.id,
        &signals,
        account.trial_seconds_remaining,
    ) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            %error,
            "failed to record trial grant"
        );
    }

    Ok(Json(auth_response(&state, &account)?))
}

pub async fn signup(
    State(_state): State<AppState>,
    Json(_req): Json<SignupRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    Err(err(
        StatusCode::GONE,
        "use /auth/signup/start and /auth/signup/confirm",
    ))
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

    let mut account = Account::fetch_by_email(&state.pool, &email)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "account vanished after auth",
            )
        })?;
    if account.is_temporary_expired() {
        return Err(err(StatusCode::UNAUTHORIZED, "temporary account expired"));
    }
    if state.config.is_admin_email(&email) && !account.is_admin {
        Account::set_admin(&state.pool, &account.id, true)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
        account.is_admin = true;
    }

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
    let account_id = refresh_tokens::consume(&state.pool, &req.refresh_token)
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
    if account.is_temporary_expired() {
        return Err(err(StatusCode::UNAUTHORIZED, "temporary account expired"));
    }

    // 3. Issue a new pair (the old token is already revoked atomically above).
    Ok(Json(auth_response(&state, &account)?))
}

pub async fn logout(
    State(state): State<AppState>,
    axum::Extension(crate::auth::AuthedAccount(account)): axum::Extension<
        crate::auth::AuthedAccount,
    >,
    Json(req): Json<LogoutRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if req.revoke_all.unwrap_or(false) {
        refresh_tokens::revoke_all_for_account(&state.pool, &account.id)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
        return Ok(StatusCode::OK);
    }

    if let Some(refresh_token) = req
        .refresh_token
        .as_deref()
        .filter(|token| !token.is_empty())
    {
        if let Ok(claims) = auth::jwt::verify(&state.config.jwt_secret, refresh_token) {
            if claims.kind == "refresh" && claims.sub == account.id {
                refresh_tokens::revoke(&state.pool, refresh_token)
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
            }
        }
    }

    Ok(StatusCode::OK)
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
    device_codes::insert(&state.pool, &device_code, &user_code, &expires_at)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    let verification_uri = format!(
        "{}/login?user_code={}",
        state.config.public_url.trim_end_matches('/'),
        user_code
    );
    Ok(Json(DeviceStartResponse {
        device_code,
        user_code,
        verification_uri,
        expires_in: DEVICE_CODE_TTL_SECS,
        interval: DEVICE_POLL_INTERVAL_SECS,
    }))
}

pub async fn device_poll(
    State(state): State<AppState>,
    Json(req): Json<DevicePollRequest>,
) -> Result<Json<AuthResponse>, (StatusCode, Json<ApiError>)> {
    let Some(row) = device_codes::fetch_by_device_code(&state.pool, &req.device_code)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
    else {
        return Err(err(StatusCode::NOT_FOUND, "unknown device_code"));
    };
    let exp: chrono::DateTime<chrono::Utc> = row
        .expires_at
        .parse()
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "bad expires_at"))?;
    if exp < chrono::Utc::now() {
        return Err(err(StatusCode::GONE, "device_code expired"));
    }
    if row.approved == 0 {
        return Err(err(StatusCode::ACCEPTED, "authorization_pending"));
    }
    let Some(account_id) = row.account_id else {
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "approved but no account",
        ));
    };

    // Consume the device code before issuing tokens. The conditional delete
    // closes the retry race where two pollers select the same approved row
    // before either one deletes it.
    if !device_codes::consume_approved(&state.pool, &req.device_code, &account_id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
    {
        return Err(err(StatusCode::GONE, "device_code already consumed"));
    }

    // Fetch account and issue tokens.
    let account = Account::fetch_by_id(&state.pool, &account_id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
        .ok_or_else(|| err(StatusCode::INTERNAL_SERVER_ERROR, "account not found"))?;
    if account.is_temporary_expired() {
        return Err(err(StatusCode::UNAUTHORIZED, "temporary account expired"));
    }
    Ok(Json(auth_response(&state, &account)?))
}

pub async fn device_approve(
    State(state): State<AppState>,
    axum::Extension(crate::auth::AuthedAccount(account)): axum::Extension<
        crate::auth::AuthedAccount,
    >,
    Json(req): Json<DeviceApproveRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    // Bind the device_code to the *currently authenticated* customer's account.
    // The auth middleware has already verified the JWT and loaded the account;
    // we use that account_id to mark the device approved.
    let now = chrono::Utc::now().to_rfc3339();
    if !device_codes::approve_user_code(&state.pool, &account.id, &req.user_code, &now)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?
    {
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
            tracing::info!(account_id_hash = %cue_core::account_id_hash_prefix(&account.id), "email verification sent");
        }
        Ok(crate::mail::MailDelivery::NotConfigured) => {
            if allow_dev_auth_link_logs() {
                tracing::info!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    verify_url = %verify_url,
                    "email verification (SMTP unconfigured; dev link logging enabled)"
                );
            } else {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    "email verification link created but SMTP is unconfigured; link suppressed from logs"
                );
            }
        }
        Err(error) => {
            tracing::warn!(account_id_hash = %cue_core::account_id_hash_prefix(&account.id), %error, "email verification delivery failed");
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
    Account::mark_email_verified(&state.pool, &account_id).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("db: {e}"),
            }),
        )
    })?;
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
                    tracing::info!(account_id_hash = %cue_core::account_id_hash_prefix(&account.id), "password reset sent");
                }
                Ok(crate::mail::MailDelivery::NotConfigured) => {
                    if allow_dev_auth_link_logs() {
                        tracing::info!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            reset_url = %reset_url,
                            "password reset (SMTP unconfigured; dev link logging enabled)"
                        );
                    } else {
                        tracing::warn!(
                            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                            "password reset link created but SMTP is unconfigured; link suppressed from logs"
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(account_id_hash = %cue_core::account_id_hash_prefix(&account.id), %error, "password reset delivery failed");
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
    Account::update_password_hash(&state.pool, &account_id, &new_hash).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("db: {e}"),
            }),
        )
    })?;
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
    use crate::auth::jwt;
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
    refresh_tokens::store(&state.pool, &refresh_raw, &account.id, Some("device-link")).map_err(
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
    if account.is_temporary_expired() {
        return Err(err(StatusCode::UNAUTHORIZED, "temporary account expired"));
    }

    Ok(Json(LinkExchangeResponse {
        access_token: access,
        refresh_token: refresh,
        account: AuthAccountSummary {
            id: account.id,
            email: account.email,
            balance_cents: account.balance_cents,
            trial_seconds_remaining: account.trial_seconds_remaining,
            is_temporary: account.is_temporary,
            temporary_expires_at: account.temporary_expires_at,
            is_admin: account.is_admin,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn signup_signals_do_not_trust_forwarded_headers_without_peer_info() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.9, 198.51.100.7"),
        );
        headers.insert("cf-connecting-ip", HeaderValue::from_static("203.0.113.10"));

        let signals = signup_signals("user@example.com", &headers, None, None);

        assert_eq!(signals.ip_hash, None);
    }
}
