//! OAuth authorization for Bluey Jobs mailbox and calendar connections.
//!
//! Provider credentials are persisted only in the server-only encrypted
//! credential table. Customer workspace responses expose connection metadata,
//! never access or refresh tokens.

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{self, JobsOAuthState, JobsProviderCredential, MailboxConnection},
    jobs_provider_auth::{
        bounded_provider_json, capabilities_for_granted_scopes, env_flag_enabled,
        normalized_scopes, provider_config, valid_provider_token, ProviderAuthorizationPurpose,
        ProviderConfig,
    },
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
const COMMUNICATION_WRITE_OAUTH_FLAG: &str = "BLUEY_JOBS_COMMUNICATION_OAUTH_WRITE_ENABLED";
const MAX_OAUTH_STATE_BYTES: usize = 256;
const MAX_OAUTH_CODE_BYTES: usize = 8 * 1024;
const MAX_PROVIDER_SUBJECT_BYTES: usize = 512;

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
    let write_enabled = env_flag_enabled(COMMUNICATION_WRITE_OAUTH_FLAG);
    Json(
        ["gmail", "outlook"]
            .into_iter()
            .map(|provider| MailboxProviderAvailability {
                provider,
                configured: provider_config(provider, ProviderAuthorizationPurpose::MailboxRead)
                    .is_some(),
                capabilities: if write_enabled
                    && provider_config(provider, ProviderAuthorizationPurpose::CommunicationWrite)
                        .is_some()
                {
                    vec![
                        "status_sync",
                        "application_correlation",
                        "review_interventions",
                        "recruiter_reply",
                        "interview_calendar",
                    ]
                } else {
                    vec![
                        "status_sync",
                        "application_correlation",
                        "review_interventions",
                    ]
                },
            })
            .collect(),
    )
}

pub async fn start_oauth(
    State(state): State<AppState>,
    Extension(account): Extension<AuthedAccount>,
    Path(provider): Path<String>,
) -> Result<Json<MailboxOAuthStart>, ApiError> {
    let config =
        provider_config(&provider, ProviderAuthorizationPurpose::MailboxRead).ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("{provider} connection is not configured"),
            )
        })?;
    begin_oauth(&state, &account.0.id, &config, None, 0).await
}

pub async fn start_communication_oauth(
    State(state): State<AppState>,
    Extension(account): Extension<AuthedAccount>,
    Path(connection_id): Path<String>,
) -> Result<Json<MailboxOAuthStart>, ApiError> {
    if !env_flag_enabled(COMMUNICATION_WRITE_OAUTH_FLAG) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "reviewed communication authorization is disabled".to_string(),
        ));
    }
    let connection = jobs::mailbox_connection(&state.pool, &account.0.id, &connection_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "mailbox connection not found".to_string(),
            )
        })?;
    let credential = jobs::jobs_provider_credential(&state.pool, &account.0.id, &connection_id)
        .map_err(internal_error)?
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "mailbox authorization needs attention".to_string(),
            )
        })?;
    if connection.status != "connected" || credential.provider != connection.provider {
        return Err((
            StatusCode::CONFLICT,
            "mailbox authorization needs attention".to_string(),
        ));
    }
    let config = provider_config(
        &connection.provider,
        ProviderAuthorizationPurpose::CommunicationWrite,
    )
    .ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider communication authorization is not configured".to_string(),
        )
    })?;
    begin_oauth(
        &state,
        &account.0.id,
        &config,
        Some(connection_id),
        credential.grant_revision,
    )
    .await
}

async fn begin_oauth(
    state: &AppState,
    account_id: &str,
    config: &ProviderConfig,
    connection_id: Option<String>,
    expected_grant_revision: i64,
) -> Result<Json<MailboxOAuthStart>, ApiError> {
    let state_token = random_url_token(32);
    let code_verifier = random_url_token(48);
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(code_verifier.as_bytes()));
    let now = now_ms();
    let requested_scopes = normalized_scopes(config.scopes.iter().copied());
    let requested_capabilities =
        capabilities_for_granted_scopes(config.provider, &requested_scopes);
    let oauth_state = JobsOAuthState {
        provider: config.provider.to_string(),
        code_verifier,
        connection_id,
        authorization_purpose: config.purpose.as_str().to_string(),
        requested_scopes,
        requested_capabilities,
        expected_grant_revision,
        return_path: "/jobs/settings".to_string(),
        expires_at_ms: now + OAUTH_STATE_TTL_MS,
        created_at_ms: now,
    };
    jobs::save_jobs_oauth_state(&state.pool, account_id, &state_token, &oauth_state)
        .map_err(internal_error)?;

    let redirect_uri = oauth_redirect_uri(state, config.provider);
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
    let (account_id, oauth_state, code) =
        consume_oauth_callback_authority(&state.pool, provider, &query)?;
    complete_consumed_oauth(state, provider, account_id, oauth_state, &code).await
}

fn consume_oauth_callback_authority(
    pool: &crate::db::DbPool,
    provider: &str,
    query: &OAuthCallbackQuery,
) -> Result<(String, JobsOAuthState, String), ApiError> {
    let state_token = query
        .state
        .as_deref()
        .filter(|value| bounded_opaque(value, MAX_OAUTH_STATE_BYTES))
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "missing OAuth state".to_string()))?;
    let (account_id, oauth_state) = jobs::consume_jobs_oauth_state(pool, state_token)
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
    if query.error.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "provider declined authorization".to_string(),
        ));
    }
    let code = query
        .code
        .as_deref()
        .filter(|value| bounded_opaque(value, MAX_OAUTH_CODE_BYTES))
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "missing OAuth code".to_string()))?
        .to_string();
    Ok((account_id, oauth_state, code))
}

async fn complete_consumed_oauth(
    state: &AppState,
    provider: &str,
    account_id: String,
    oauth_state: JobsOAuthState,
    code: &str,
) -> Result<MailboxConnection, ApiError> {
    let purpose = ProviderAuthorizationPurpose::parse(&oauth_state.authorization_purpose)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "authorization purpose is invalid".to_string(),
            )
        })?;
    if purpose == ProviderAuthorizationPurpose::CommunicationWrite
        && !env_flag_enabled(COMMUNICATION_WRITE_OAUTH_FLAG)
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "reviewed communication authorization is disabled".to_string(),
        ));
    }
    let config = provider_config(provider, purpose).ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "provider connection is not configured".to_string(),
        )
    })?;
    let configured_scopes = normalized_scopes(config.scopes.iter().copied());
    if oauth_state.requested_scopes != configured_scopes
        || oauth_state.requested_capabilities
            != capabilities_for_granted_scopes(provider, &configured_scopes)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "authorization request no longer matches provider policy".to_string(),
        ));
    }
    let redirect_uri = oauth_redirect_uri(state, provider);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(internal_error)?;
    let token_response = client
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
        .map_err(provider_error)?;
    if !token_response.status().is_success() {
        return Err(provider_error("provider token exchange was rejected"));
    }
    let token = bounded_provider_json::<TokenResponse>(token_response)
        .await
        .map_err(|_| provider_error("provider token response was invalid"))?;
    if !valid_provider_token(&token.access_token)
        || (!token.refresh_token.is_empty() && !valid_provider_token(&token.refresh_token))
    {
        return Err(provider_error("provider token response was invalid"));
    }

    let (provider_subject, account_label) =
        provider_profile(&client, provider, &token.access_token).await?;
    if !bounded_opaque(&provider_subject, MAX_PROVIDER_SUBJECT_BYTES) {
        return Err(provider_error("provider profile response was invalid"));
    }
    let existing_mailbox = jobs::mailbox_connection_for_provider_subject(
        &state.pool,
        &account_id,
        provider,
        &provider_subject,
    )
    .map_err(internal_error)?;
    if let Some(bound_connection_id) = oauth_state.connection_id.as_deref() {
        if existing_mailbox.as_ref().map(|mailbox| mailbox.id.as_str()) != Some(bound_connection_id)
        {
            return Err((
                StatusCode::CONFLICT,
                "authorization does not match the connected mailbox".to_string(),
            ));
        }
        if existing_mailbox
            .as_ref()
            .map(|mailbox| mailbox.status.as_str())
            != Some("connected")
        {
            return Err((
                StatusCode::CONFLICT,
                "mailbox connection changed while consent was pending".to_string(),
            ));
        }
    } else if purpose == ProviderAuthorizationPurpose::CommunicationWrite {
        return Err((
            StatusCode::BAD_REQUEST,
            "communication authorization is not connection-bound".to_string(),
        ));
    }
    let existing = if let Some(mailbox) = existing_mailbox.as_ref() {
        jobs::jobs_provider_credential(&state.pool, &account_id, &mailbox.id)
            .map_err(internal_error)?
    } else {
        None
    };
    if purpose == ProviderAuthorizationPurpose::CommunicationWrite
        && existing
            .as_ref()
            .map(|credential| credential.grant_revision)
            != Some(oauth_state.expected_grant_revision)
    {
        return Err((
            StatusCode::CONFLICT,
            "mailbox authorization changed while consent was pending".to_string(),
        ));
    }
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
        if purpose == ProviderAuthorizationPurpose::CommunicationWrite {
            return Err((
                StatusCode::BAD_GATEWAY,
                "provider did not confirm communication permissions".to_string(),
            ));
        }
        configured_scopes
    } else {
        normalized_scopes(token.scope.split_whitespace())
    };
    if !oauth_state.requested_scopes.iter().all(|required| {
        scopes
            .iter()
            .any(|scope| scope.eq_ignore_ascii_case(required))
    }) {
        return Err((
            StatusCode::BAD_GATEWAY,
            "provider did not grant the requested permissions".to_string(),
        ));
    }
    let capabilities = capabilities_for_granted_scopes(provider, &scopes);
    if !oauth_state
        .requested_capabilities
        .iter()
        .all(|required| capabilities.iter().any(|capability| capability == required))
    {
        return Err((
            StatusCode::BAD_GATEWAY,
            "provider grant is missing a requested capability".to_string(),
        ));
    }
    let grant_revision = match existing.as_ref() {
        Some(credential) => credential
            .grant_revision
            .max(0)
            .checked_add(1)
            .ok_or_else(|| {
                (
                    StatusCode::CONFLICT,
                    "provider grant revision is exhausted".to_string(),
                )
            })?,
        None => 0,
    };
    let mailbox_value = authorized_mailbox_value(
        existing_mailbox.as_ref(),
        provider,
        account_label,
        capabilities.clone(),
    );
    let mut credential_value = JobsProviderCredential {
        connection_id: mailbox_value.id.clone(),
        provider: provider.to_string(),
        provider_subject,
        access_token: token.access_token,
        refresh_token,
        scopes,
        capabilities,
        grant_revision,
        grant_sha256: String::new(),
        expires_at_ms: now.saturating_add(token.expires_in.max(60).saturating_mul(1_000)),
        created_at_ms: existing
            .as_ref()
            .map(|credential| credential.created_at_ms)
            .unwrap_or(now),
        updated_at_ms: now,
    };
    if !credential_value.connection_id.is_empty() && credential_value.grant_revision > 0 {
        credential_value.grant_sha256 =
            jobs::communication_grant_sha256(&credential_value).map_err(internal_error)?;
    }
    let (mailbox, _) = if purpose == ProviderAuthorizationPurpose::CommunicationWrite {
        jobs::save_mailbox_connection_with_credential_cas(
            &state.pool,
            &account_id,
            &mailbox_value,
            &credential_value,
            oauth_state.expected_grant_revision,
        )
    } else {
        jobs::save_mailbox_connection_with_credential(
            &state.pool,
            &account_id,
            &mailbox_value,
            &credential_value,
        )
    }
    .map_err(internal_error)?;
    Ok(mailbox)
}

fn authorized_mailbox_value(
    existing: Option<&MailboxConnection>,
    provider: &str,
    account_label: String,
    capabilities: Vec<String>,
) -> MailboxConnection {
    MailboxConnection {
        id: existing
            .map(|mailbox| mailbox.id.clone())
            .unwrap_or_default(),
        provider: provider.to_string(),
        status: "connected".to_string(),
        account_label,
        aliases: existing
            .map(|mailbox| mailbox.aliases.clone())
            .unwrap_or_default(),
        capabilities,
        created_at_ms: existing.map(|mailbox| mailbox.created_at_ms).unwrap_or(0),
        updated_at_ms: 0,
    }
}

async fn provider_profile(
    client: &reqwest::Client,
    provider: &str,
    access_token: &str,
) -> Result<(String, String), ApiError> {
    match provider {
        "gmail" => {
            let response = client
                .get("https://openidconnect.googleapis.com/v1/userinfo")
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(provider_error)?;
            if !response.status().is_success() {
                return Err(provider_error("provider profile request was rejected"));
            }
            let profile = bounded_provider_json::<GoogleProfile>(response)
                .await
                .map_err(|_| provider_error("provider profile response was invalid"))?;
            let email = jobs::normalize_application_email(&profile.email).map_err(bad_request)?;
            Ok((profile.sub, email))
        }
        "outlook" => {
            let response = client
                .get("https://graph.microsoft.com/v1.0/me")
                .query(&[("$select", "id,mail,userPrincipalName")])
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(provider_error)?;
            if !response.status().is_success() {
                return Err(provider_error("provider profile request was rejected"));
            }
            let profile = bounded_provider_json::<MicrosoftProfile>(response)
                .await
                .map_err(|_| provider_error("provider profile response was invalid"))?;
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

fn bounded_opaque(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
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

    fn oauth_test_pool() -> crate::db::DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-oauth-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-oauth', 'oauth@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        pool
    }

    fn oauth_state(provider: &str) -> JobsOAuthState {
        JobsOAuthState {
            provider: provider.to_string(),
            code_verifier: "verifier-that-is-long-enough-for-oauth-pkce".to_string(),
            connection_id: None,
            authorization_purpose: "mailbox_read".to_string(),
            requested_scopes: vec!["openid".to_string()],
            requested_capabilities: Vec::new(),
            expected_grant_revision: 0,
            return_path: "/jobs/settings".to_string(),
            expires_at_ms: now_ms() + 60_000,
            created_at_ms: now_ms(),
        }
    }

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

    #[test]
    fn oauth_opaque_values_are_bounded_and_control_free() {
        assert!(bounded_opaque("opaque-value", 32));
        assert!(!bounded_opaque("", 32));
        assert!(!bounded_opaque("contains\ncontrol", 32));
        assert!(!bounded_opaque(" leading", 32));
        assert!(!bounded_opaque("trailing ", 32));
        assert!(!bounded_opaque("embedded space", 32));
        assert!(!bounded_opaque("contains\tcontrol", 32));
        assert!(!bounded_opaque("contains\u{00a0}space", 32));
        assert!(!bounded_opaque(&"x".repeat(33), 32));
    }

    #[test]
    fn oauth_decline_and_provider_mismatch_consume_state_once() {
        let pool = oauth_test_pool();
        let declined_token = "A".repeat(43);
        jobs::save_jobs_oauth_state(&pool, "acct-oauth", &declined_token, &oauth_state("gmail"))
            .unwrap();
        let decline = consume_oauth_callback_authority(
            &pool,
            "gmail",
            &OAuthCallbackQuery {
                code: None,
                state: Some(declined_token.clone()),
                error: Some("access_denied secret-provider-detail".to_string()),
            },
        )
        .unwrap_err();
        assert_eq!(decline.0, StatusCode::BAD_REQUEST);
        assert_eq!(decline.1, "provider declined authorization");
        assert!(jobs::consume_jobs_oauth_state(&pool, &declined_token)
            .unwrap()
            .is_none());

        let mismatch_token = "B".repeat(43);
        jobs::save_jobs_oauth_state(&pool, "acct-oauth", &mismatch_token, &oauth_state("gmail"))
            .unwrap();
        let mismatch = consume_oauth_callback_authority(
            &pool,
            "outlook",
            &OAuthCallbackQuery {
                code: Some("provider-code".to_string()),
                state: Some(mismatch_token.clone()),
                error: None,
            },
        )
        .unwrap_err();
        assert_eq!(mismatch.1, "authorization provider mismatch");
        assert!(jobs::consume_jobs_oauth_state(&pool, &mismatch_token)
            .unwrap()
            .is_none());
    }

    #[test]
    fn oauth_missing_state_and_replay_fail_closed() {
        let pool = oauth_test_pool();
        let missing = consume_oauth_callback_authority(
            &pool,
            "gmail",
            &OAuthCallbackQuery {
                code: Some("provider-code".to_string()),
                state: None,
                error: None,
            },
        )
        .unwrap_err();
        assert_eq!(missing.1, "missing OAuth state");

        let token = "C".repeat(43);
        jobs::save_jobs_oauth_state(&pool, "acct-oauth", &token, &oauth_state("gmail")).unwrap();
        let accepted = consume_oauth_callback_authority(
            &pool,
            "gmail",
            &OAuthCallbackQuery {
                code: Some("provider-code".to_string()),
                state: Some(token.clone()),
                error: None,
            },
        )
        .unwrap();
        assert_eq!(accepted.2, "provider-code");
        let replay = consume_oauth_callback_authority(
            &pool,
            "gmail",
            &OAuthCallbackQuery {
                code: Some("provider-code".to_string()),
                state: Some(token),
                error: None,
            },
        )
        .unwrap_err();
        assert_eq!(replay.1, "authorization expired or was already used");
    }

    #[test]
    fn write_upgrade_preserves_connection_identity_and_aliases() {
        let existing = MailboxConnection {
            id: "mailbox-1".to_string(),
            provider: "gmail".to_string(),
            status: "connected".to_string(),
            account_label: "old@example.com".to_string(),
            aliases: vec!["alias@example.com".to_string()],
            capabilities: vec!["status_sync".to_string()],
            created_at_ms: 123,
            updated_at_ms: 456,
        };
        let upgraded = authorized_mailbox_value(
            Some(&existing),
            "gmail",
            "current@example.com".to_string(),
            vec!["status_sync".to_string(), "recruiter_reply".to_string()],
        );

        assert_eq!(upgraded.id, existing.id);
        assert_eq!(upgraded.created_at_ms, existing.created_at_ms);
        assert_eq!(upgraded.aliases, existing.aliases);
        assert_eq!(upgraded.account_label, "current@example.com");
        assert!(upgraded
            .capabilities
            .contains(&"recruiter_reply".to_string()));
    }
}
