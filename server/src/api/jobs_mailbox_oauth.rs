//! OAuth authorization for Bluey Jobs mailbox and calendar connections.
//!
//! Provider credentials are persisted only in the server-only encrypted
//! credential table. Customer workspace responses expose connection metadata,
//! never access or refresh tokens.

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{self, JobsOAuthState, JobsProviderCredential, MailboxConnection},
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Extension, Json, Router,
};
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type ApiError = (StatusCode, String);
const OAUTH_STATE_TTL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone)]
pub(crate) struct ProviderConfig {
    pub(crate) provider: &'static str,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) authorize_url: String,
    pub(crate) token_url: String,
    pub(crate) scopes: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct MailboxProviderAvailability {
    provider: &'static str,
    configured: bool,
    capabilities: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct MailboxOAuthStart {
    authorization_url: String,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

#[derive(Debug, Deserialize)]
struct GoogleProfile {
    sub: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct MicrosoftProfile {
    id: String,
    #[serde(default)]
    mail: String,
    #[serde(rename = "userPrincipalName", default)]
    user_principal_name: String,
}

pub fn public_router() -> Router<AppState> {
    Router::new().route("/api/jobs/oauth/:provider/callback", get(oauth_callback))
}

pub async fn provider_availability() -> Json<Vec<MailboxProviderAvailability>> {
    Json(
        ["gmail", "outlook"]
            .into_iter()
            .map(|provider| MailboxProviderAvailability {
                provider,
                configured: provider_config(provider).is_some(),
                capabilities: vec![
                    "status_sync",
                    "application_correlation",
                    "review_interventions",
                ],
            })
            .collect(),
    )
}

pub async fn start_oauth(
    State(state): State<AppState>,
    Extension(account): Extension<AuthedAccount>,
    Path(provider): Path<String>,
) -> Result<Json<MailboxOAuthStart>, ApiError> {
    let config = provider_config(&provider).ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("{provider} connection is not configured"),
        )
    })?;
    let state_token = random_url_token(32);
    let code_verifier = random_url_token(48);
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(code_verifier.as_bytes()));
    let now = now_ms();
    let oauth_state = JobsOAuthState {
        provider: config.provider.to_string(),
        code_verifier,
        return_path: "/jobs/settings".to_string(),
        expires_at_ms: now + OAUTH_STATE_TTL_MS,
        created_at_ms: now,
    };
    jobs::save_jobs_oauth_state(&state.pool, &account.0.id, &state_token, &oauth_state)
        .map_err(internal_error)?;

    let redirect_uri = oauth_redirect_uri(&state, config.provider);
    let mut url = reqwest::Url::parse(&config.authorize_url).map_err(internal_error)?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("client_id", &config.client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &config.scopes.join(" "))
            .append_pair("state", &state_token)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256");
        if config.provider == "gmail" {
            query
                .append_pair("access_type", "offline")
                .append_pair("include_granted_scopes", "true")
                .append_pair("prompt", "consent");
        } else {
            query.append_pair("response_mode", "query");
        }
    }
    Ok(Json(MailboxOAuthStart {
        authorization_url: url.to_string(),
    }))
}

async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    match complete_oauth(&state, &provider, query).await {
        Ok(connection) => settings_redirect(
            &state,
            &[
                ("mailbox", "connected"),
                ("provider", connection.provider.as_str()),
            ],
        )
        .into_response(),
        Err((status, message)) => {
            tracing::warn!(
                provider = %provider,
                status = status.as_u16(),
                "Jobs mailbox authorization failed"
            );
            settings_redirect(
                &state,
                &[
                    ("mailbox", "error"),
                    ("reason", public_oauth_error(&message)),
                ],
            )
            .into_response()
        }
    }
}

async fn complete_oauth(
    state: &AppState,
    provider: &str,
    query: OAuthCallbackQuery,
) -> Result<MailboxConnection, ApiError> {
    if let Some(error) = query.error.as_deref() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("provider declined authorization: {error}"),
        ));
    }
    let state_token = query
        .state
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "missing OAuth state".to_string()))?;
    let (account_id, oauth_state) = jobs::consume_jobs_oauth_state(&state.pool, state_token)
        .map_err(internal_error)?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "authorization expired or was already used".to_string(),
            )
        })?;
    if oauth_state.provider != provider {
        return Err((
            StatusCode::BAD_REQUEST,
            "authorization provider mismatch".to_string(),
        ));
    }
    let code = query
        .code
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "missing OAuth code".to_string()))?;
    let config = provider_config(provider).ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider connection is not configured".to_string(),
        )
    })?;
    let redirect_uri = oauth_redirect_uri(state, provider);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(internal_error)?;
    let token = client
        .post(&config.token_url)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code", code),
            ("code_verifier", oauth_state.code_verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(provider_error)?
        .error_for_status()
        .map_err(provider_error)?
        .json::<TokenResponse>()
        .await
        .map_err(provider_error)?;

    let (provider_subject, account_label) =
        provider_profile(&client, provider, &token.access_token).await?;
    let existing_mailbox = jobs::mailbox_connection_for_provider_subject(
        &state.pool,
        &account_id,
        provider,
        &provider_subject,
    )
    .map_err(internal_error)?;
    let existing = if let Some(mailbox) = existing_mailbox.as_ref() {
        jobs::jobs_provider_credential(&state.pool, &account_id, &mailbox.id)
            .map_err(internal_error)?
    } else {
        None
    };
    let now = now_ms();
    let refresh_token = if token.refresh_token.is_empty() {
        existing
            .as_ref()
            .map(|credential| credential.refresh_token.clone())
            .unwrap_or_default()
    } else {
        token.refresh_token
    };
    if refresh_token.is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "provider did not issue offline access".to_string(),
        ));
    }
    let scopes = if token.scope.trim().is_empty() {
        config
            .scopes
            .iter()
            .map(|scope| (*scope).to_string())
            .collect()
    } else {
        token.scope.split_whitespace().map(str::to_string).collect()
    };
    let (mailbox, _) = jobs::save_mailbox_connection_with_credential(
        &state.pool,
        &account_id,
        &MailboxConnection {
            id: existing_mailbox
                .as_ref()
                .map(|mailbox| mailbox.id.clone())
                .unwrap_or_default(),
            provider: provider.to_string(),
            status: "connected".to_string(),
            account_label,
            aliases: Vec::new(),
            capabilities: vec![
                "status_sync".to_string(),
                "application_correlation".to_string(),
                "review_interventions".to_string(),
            ],
            created_at_ms: existing_mailbox
                .as_ref()
                .map(|mailbox| mailbox.created_at_ms)
                .unwrap_or(0),
            updated_at_ms: 0,
        },
        &JobsProviderCredential {
            connection_id: existing_mailbox
                .as_ref()
                .map(|mailbox| mailbox.id.clone())
                .unwrap_or_default(),
            provider: provider.to_string(),
            provider_subject,
            access_token: token.access_token,
            refresh_token,
            scopes,
            expires_at_ms: now + token.expires_in.max(60) * 1000,
            created_at_ms: existing
                .as_ref()
                .map(|credential| credential.created_at_ms)
                .unwrap_or(now),
            updated_at_ms: now,
        },
    )
    .map_err(internal_error)?;
    Ok(mailbox)
}

async fn provider_profile(
    client: &reqwest::Client,
    provider: &str,
    access_token: &str,
) -> Result<(String, String), ApiError> {
    match provider {
        "gmail" => {
            let profile = client
                .get("https://openidconnect.googleapis.com/v1/userinfo")
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(provider_error)?
                .error_for_status()
                .map_err(provider_error)?
                .json::<GoogleProfile>()
                .await
                .map_err(provider_error)?;
            let email = jobs::normalize_application_email(&profile.email).map_err(bad_request)?;
            Ok((profile.sub, email))
        }
        "outlook" => {
            let profile = client
                .get("https://graph.microsoft.com/v1.0/me")
                .query(&[("$select", "id,mail,userPrincipalName")])
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(provider_error)?
                .error_for_status()
                .map_err(provider_error)?
                .json::<MicrosoftProfile>()
                .await
                .map_err(provider_error)?;
            let email = if profile.mail.trim().is_empty() {
                profile.user_principal_name
            } else {
                profile.mail
            };
            Ok((
                profile.id,
                jobs::normalize_application_email(&email).map_err(bad_request)?,
            ))
        }
        _ => Err((StatusCode::BAD_REQUEST, "unsupported provider".to_string())),
    }
}

pub(crate) fn provider_config(provider: &str) -> Option<ProviderConfig> {
    let required = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    match provider {
        "gmail" => Some(ProviderConfig {
            provider: "gmail",
            client_id: required("BLUEY_JOBS_GOOGLE_CLIENT_ID")?,
            client_secret: required("BLUEY_JOBS_GOOGLE_CLIENT_SECRET")?,
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: vec![
                "openid",
                "email",
                "https://www.googleapis.com/auth/gmail.readonly",
            ],
        }),
        "outlook" => {
            let tenant =
                required("BLUEY_JOBS_MICROSOFT_TENANT_ID").unwrap_or_else(|| "common".to_string());
            Some(ProviderConfig {
                provider: "outlook",
                client_id: required("BLUEY_JOBS_MICROSOFT_CLIENT_ID")?,
                client_secret: required("BLUEY_JOBS_MICROSOFT_CLIENT_SECRET")?,
                authorize_url: format!(
                    "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"
                ),
                token_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
                scopes: vec![
                    "openid",
                    "email",
                    "offline_access",
                    "User.Read",
                    "Mail.Read",
                ],
            })
        }
        _ => None,
    }
}

fn oauth_redirect_uri(state: &AppState, provider: &str) -> String {
    format!(
        "{}/api/jobs/oauth/{provider}/callback",
        state.config.public_url.trim_end_matches('/')
    )
}

fn settings_redirect(state: &AppState, pairs: &[(&str, &str)]) -> Redirect {
    let base = format!(
        "{}/jobs/settings",
        state.config.public_url.trim_end_matches('/')
    );
    let mut url = reqwest::Url::parse(&base).expect("configured public URL");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    Redirect::to(url.as_str())
}

fn public_oauth_error(message: &str) -> &'static str {
    if message.contains("expired") {
        "expired"
    } else if message.contains("declined") {
        "declined"
    } else if message.contains("limit") {
        "plan_limit"
    } else {
        "provider_error"
    }
}

fn random_url_token(bytes: usize) -> String {
    let mut value = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut value);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, "Jobs OAuth internal error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "could not complete provider connection".to_string(),
    )
}

fn provider_error(error: impl std::fmt::Display) -> ApiError {
    tracing::warn!(error = %error, "Jobs OAuth provider request failed");
    (
        StatusCode::BAD_GATEWAY,
        "provider authorization could not be completed".to_string(),
    )
}

fn bad_request(error: impl std::fmt::Display) -> ApiError {
    (StatusCode::BAD_REQUEST, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_is_publicly_coarsened() {
        assert_eq!(public_oauth_error("authorization expired"), "expired");
        assert_eq!(public_oauth_error("provider declined"), "declined");
        assert_eq!(public_oauth_error("inbox limit reached"), "plan_limit");
        assert_eq!(public_oauth_error("secret detail"), "provider_error");
    }

    #[test]
    fn random_tokens_are_url_safe_and_unique() {
        let first = random_url_token(32);
        let second = random_url_token(32);
        assert_ne!(first, second);
        assert!(first.len() >= 43);
        assert!(!first.contains('='));
    }
}
