//! Server-only OAuth configuration and token refresh for Bluey Jobs providers.
//!
//! Provider client secrets and user credentials never leave `bluey-server`.

use serde::{de::DeserializeOwned, Deserialize};

const MAX_PROVIDER_JSON_BYTES: usize = 256 * 1024;
pub(crate) const MAX_PROVIDER_TOKEN_BYTES: usize = 16 * 1024;

pub(crate) const GOOGLE_GMAIL_READ_SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
pub(crate) const GOOGLE_GMAIL_SEND_SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";
pub(crate) const GOOGLE_CALENDAR_EVENTS_SCOPE: &str =
    "https://www.googleapis.com/auth/calendar.events";
pub(crate) const MICROSOFT_MAIL_READ_SCOPE: &str = "Mail.Read";
pub(crate) const MICROSOFT_MAIL_SEND_SCOPE: &str = "Mail.Send";
pub(crate) const MICROSOFT_CALENDAR_WRITE_SCOPE: &str = "Calendars.ReadWrite";

pub(crate) const CAPABILITY_STATUS_SYNC: &str = "status_sync";
pub(crate) const CAPABILITY_APPLICATION_CORRELATION: &str = "application_correlation";
pub(crate) const CAPABILITY_REVIEW_INTERVENTIONS: &str = "review_interventions";
pub(crate) const CAPABILITY_RECRUITER_REPLY: &str = "recruiter_reply";
pub(crate) const CAPABILITY_INTERVIEW_CALENDAR: &str = "interview_calendar";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderAuthorizationPurpose {
    MailboxRead,
    CommunicationWrite,
}

impl ProviderAuthorizationPurpose {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MailboxRead => "mailbox_read",
            Self::CommunicationWrite => "communication_write",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "" | "mailbox_read" => Some(Self::MailboxRead),
            "communication_write" => Some(Self::CommunicationWrite),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderConfig {
    pub(crate) provider: &'static str,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) authorize_url: String,
    pub(crate) token_url: String,
    pub(crate) scopes: Vec<&'static str>,
    pub(crate) purpose: ProviderAuthorizationPurpose,
}

#[derive(Debug)]
pub(crate) struct RefreshResult {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) scopes: Vec<String>,
    pub(crate) expires_in_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderAuthErrorKind {
    ReauthorizationRequired,
    Transient,
}

#[derive(Debug, thiserror::Error)]
#[error("{public_message}")]
pub(crate) struct ProviderAuthError {
    kind: ProviderAuthErrorKind,
    public_message: &'static str,
}

impl ProviderAuthError {
    pub(crate) fn kind(&self) -> ProviderAuthErrorKind {
        self.kind
    }

    fn reauthorization() -> Self {
        Self {
            kind: ProviderAuthErrorKind::ReauthorizationRequired,
            public_message: "mailbox authorization needs attention",
        }
    }

    fn transient() -> Self {
        Self {
            kind: ProviderAuthErrorKind::Transient,
            public_message: "provider token refresh did not complete",
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: i64,
    #[serde(default)]
    scope: String,
}

#[derive(Default, Deserialize)]
struct TokenErrorResponse {
    #[serde(default)]
    error: String,
}

pub(crate) fn provider_config(
    provider: &str,
    purpose: ProviderAuthorizationPurpose,
) -> Option<ProviderConfig> {
    let required = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    match provider {
        "gmail" => {
            let mut scopes = vec!["openid", "email", GOOGLE_GMAIL_READ_SCOPE];
            if purpose == ProviderAuthorizationPurpose::CommunicationWrite {
                scopes.extend([GOOGLE_GMAIL_SEND_SCOPE, GOOGLE_CALENDAR_EVENTS_SCOPE]);
            }
            Some(ProviderConfig {
                provider: "gmail",
                client_id: required("BLUEY_JOBS_GOOGLE_CLIENT_ID")?,
                client_secret: required("BLUEY_JOBS_GOOGLE_CLIENT_SECRET")?,
                authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                scopes,
                purpose,
            })
        }
        "outlook" => {
            let tenant =
                required("BLUEY_JOBS_MICROSOFT_TENANT_ID").unwrap_or_else(|| "common".to_string());
            let mut scopes = vec!["openid", "email", "offline_access", "User.Read"];
            scopes.push(MICROSOFT_MAIL_READ_SCOPE);
            if purpose == ProviderAuthorizationPurpose::CommunicationWrite {
                scopes.extend([MICROSOFT_MAIL_SEND_SCOPE, MICROSOFT_CALENDAR_WRITE_SCOPE]);
            }
            Some(ProviderConfig {
                provider: "outlook",
                client_id: required("BLUEY_JOBS_MICROSOFT_CLIENT_ID")?,
                client_secret: required("BLUEY_JOBS_MICROSOFT_CLIENT_SECRET")?,
                authorize_url: format!(
                    "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"
                ),
                token_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
                scopes,
                purpose,
            })
        }
        _ => None,
    }
}

pub(crate) async fn refresh_access_token(
    client: &reqwest::Client,
    config: &ProviderConfig,
    refresh_token: &str,
) -> Result<RefreshResult, ProviderAuthError> {
    if refresh_token.trim().is_empty() {
        return Err(ProviderAuthError::reauthorization());
    }
    let response = client
        .post(&config.token_url)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", config.scopes.join(" ").as_str()),
        ])
        .send()
        .await
        .map_err(|_| ProviderAuthError::transient())?;
    let status = response.status();
    if !status.is_success() {
        let error = bounded_provider_json::<TokenErrorResponse>(response)
            .await
            .unwrap_or_default();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
            || error.error.eq_ignore_ascii_case("invalid_grant")
        {
            return Err(ProviderAuthError::reauthorization());
        }
        return Err(ProviderAuthError::transient());
    }
    let token = bounded_provider_json::<TokenResponse>(response)
        .await
        .map_err(|_| ProviderAuthError::transient())?;
    if !valid_provider_token(&token.access_token)
        || (!token.refresh_token.is_empty() && !valid_provider_token(&token.refresh_token))
    {
        return Err(ProviderAuthError::transient());
    }
    Ok(RefreshResult {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        scopes: normalized_scopes(token.scope.split_whitespace()),
        expires_in_seconds: token.expires_in,
    })
}

pub(crate) fn valid_provider_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER_TOKEN_BYTES
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

pub(crate) async fn bounded_provider_json<T: DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, ()> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PROVIDER_JSON_BYTES as u64)
    {
        return Err(());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_JSON_BYTES {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| ())
}

pub(crate) fn normalized_scopes<'a>(scopes: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut values = scopes
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.to_ascii_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn has_scope(scopes: &[String], required: &str) -> bool {
    scopes
        .iter()
        .any(|scope| scope.trim().eq_ignore_ascii_case(required))
}

pub(crate) fn capabilities_for_granted_scopes(provider: &str, scopes: &[String]) -> Vec<String> {
    let mut capabilities = Vec::new();
    let has_read = match provider {
        "gmail" => has_scope(scopes, GOOGLE_GMAIL_READ_SCOPE),
        "outlook" => has_scope(scopes, MICROSOFT_MAIL_READ_SCOPE),
        _ => false,
    };
    if has_read {
        capabilities.extend([
            CAPABILITY_STATUS_SYNC.to_string(),
            CAPABILITY_APPLICATION_CORRELATION.to_string(),
            CAPABILITY_REVIEW_INTERVENTIONS.to_string(),
        ]);
    }
    let can_reply = match provider {
        "gmail" => has_scope(scopes, GOOGLE_GMAIL_SEND_SCOPE),
        "outlook" => has_scope(scopes, MICROSOFT_MAIL_SEND_SCOPE),
        _ => false,
    };
    if can_reply {
        capabilities.push(CAPABILITY_RECRUITER_REPLY.to_string());
    }
    let can_create_event = match provider {
        "gmail" => has_scope(scopes, GOOGLE_CALENDAR_EVENTS_SCOPE),
        "outlook" => has_scope(scopes, MICROSOFT_CALENDAR_WRITE_SCOPE),
        _ => false,
    };
    if can_create_event {
        capabilities.push(CAPABILITY_INTERVIEW_CALENDAR.to_string());
    }
    capabilities
}

pub(crate) fn action_provider_is_granted(
    connection_provider: &str,
    action_provider: &str,
    scopes: &[String],
    capabilities: &[String],
) -> bool {
    let (expected_connection, required_scope, required_capability) = match action_provider {
        "gmail" => ("gmail", GOOGLE_GMAIL_SEND_SCOPE, CAPABILITY_RECRUITER_REPLY),
        "outlook_email" => (
            "outlook",
            MICROSOFT_MAIL_SEND_SCOPE,
            CAPABILITY_RECRUITER_REPLY,
        ),
        "google_calendar" => (
            "gmail",
            GOOGLE_CALENDAR_EVENTS_SCOPE,
            CAPABILITY_INTERVIEW_CALENDAR,
        ),
        "outlook_calendar" => (
            "outlook",
            MICROSOFT_CALENDAR_WRITE_SCOPE,
            CAPABILITY_INTERVIEW_CALENDAR,
        ),
        _ => return false,
    };
    connection_provider == expected_connection
        && has_scope(scopes, required_scope)
        && capabilities
            .iter()
            .any(|capability| capability == required_capability)
}

pub(crate) fn action_provider_lookup_is_granted(
    connection_provider: &str,
    action_provider: &str,
    scopes: &[String],
) -> bool {
    let (expected_connection, required_scope) = match action_provider {
        "gmail" => ("gmail", GOOGLE_GMAIL_READ_SCOPE),
        "outlook_email" => ("outlook", MICROSOFT_MAIL_READ_SCOPE),
        "google_calendar" => ("gmail", GOOGLE_CALENDAR_EVENTS_SCOPE),
        "outlook_calendar" => ("outlook", MICROSOFT_CALENDAR_WRITE_SCOPE),
        _ => return false,
    };
    connection_provider == expected_connection && has_scope(scopes, required_scope)
}

pub(crate) fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[test]
    fn granted_capabilities_are_derived_from_exact_scopes() {
        let read_only = vec![GOOGLE_GMAIL_READ_SCOPE.to_string()];
        assert_eq!(
            capabilities_for_granted_scopes("gmail", &read_only),
            vec![
                CAPABILITY_STATUS_SYNC,
                CAPABILITY_APPLICATION_CORRELATION,
                CAPABILITY_REVIEW_INTERVENTIONS,
            ]
        );
        assert!(!action_provider_is_granted(
            "gmail",
            "gmail",
            &read_only,
            &[CAPABILITY_RECRUITER_REPLY.to_string()],
        ));

        let write = vec![
            GOOGLE_GMAIL_READ_SCOPE.to_string(),
            GOOGLE_GMAIL_SEND_SCOPE.to_string(),
            GOOGLE_CALENDAR_EVENTS_SCOPE.to_string(),
        ];
        let capabilities = capabilities_for_granted_scopes("gmail", &write);
        assert!(action_provider_is_granted(
            "gmail",
            "gmail",
            &write,
            &capabilities,
        ));
        assert!(action_provider_is_granted(
            "gmail",
            "google_calendar",
            &write,
            &capabilities,
        ));
        assert!(!action_provider_is_granted(
            "gmail",
            "outlook_email",
            &write,
            &capabilities,
        ));
        assert!(action_provider_lookup_is_granted("gmail", "gmail", &write,));
        assert!(action_provider_lookup_is_granted(
            "gmail",
            "google_calendar",
            &write,
        ));
        assert!(!action_provider_lookup_is_granted(
            "gmail",
            "outlook_email",
            &write,
        ));
    }

    #[test]
    fn flags_are_disabled_unless_explicitly_enabled() {
        const FLAG: &str = "BLUEY_TEST_COMMUNICATION_AUTH_FLAG";
        std::env::remove_var(FLAG);
        assert!(!env_flag_enabled(FLAG));
        std::env::set_var(FLAG, " yes ");
        assert!(env_flag_enabled(FLAG));
        std::env::remove_var(FLAG);
    }

    #[test]
    fn refreshed_tokens_are_bounded_and_control_free() {
        assert!(valid_provider_token("opaque-token"));
        assert!(valid_provider_token(&"x".repeat(MAX_PROVIDER_TOKEN_BYTES)));
        assert!(!valid_provider_token(""));
        assert!(!valid_provider_token("opaque\ntoken"));
        assert!(!valid_provider_token("opaque token"));
        assert!(!valid_provider_token(
            &"x".repeat(MAX_PROVIDER_TOKEN_BYTES + 1)
        ));
    }

    #[tokio::test]
    async fn provider_json_reader_rejects_oversized_responses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("x".repeat(MAX_PROVIDER_JSON_BYTES + 1), "application/json"),
            )
            .mount(&server)
            .await;
        let response = reqwest::Client::new()
            .get(server.uri())
            .send()
            .await
            .unwrap();
        assert!(bounded_provider_json::<serde_json::Value>(response)
            .await
            .is_err());
    }
}
