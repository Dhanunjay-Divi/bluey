//! Provider configuration — pure data, no I/O.
//!
//! Endpoints, scopes, and the PUBLIC client id for each supported OAuth
//! calendar provider. Client ids are public identifiers (there is NO client
//! secret in the native public-client flow), so they are baked into the build,
//! overridable at compile time via `BLUEY_GOOGLE_CLIENT_ID` /
//! `BLUEY_MICROSOFT_CLIENT_ID` so the human can drop in real ids later.

/// Compile-time client-id fallbacks. These are placeholders; the real public
/// client ids are injected at build time via the matching env vars.
const GOOGLE_CLIENT_ID: &str = match option_env!("BLUEY_GOOGLE_CLIENT_ID") {
    Some(id) => id,
    None => "PLACEHOLDER_GOOGLE_CLIENT_ID",
};
const MICROSOFT_CLIENT_ID: &str = match option_env!("BLUEY_MICROSOFT_CLIENT_ID") {
    Some(id) => id,
    None => "PLACEHOLDER_MICROSOFT_CLIENT_ID",
};

/// The supported cloud calendar providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Google,
    Microsoft,
}

/// Static configuration for one provider's OAuth + calendar endpoints.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Authorization endpoint (where the browser is sent for consent).
    pub authorize_url: String,
    /// Token endpoint (code exchange + refresh).
    pub token_url: String,
    /// Requested OAuth scope string (space-delimited).
    pub scope: String,
    /// PUBLIC OAuth client id (not a secret).
    pub client_id: String,
    /// Extra params appended to the authorize URL (e.g. Google's
    /// `access_type=offline` + `prompt=consent` to guarantee a refresh token).
    pub extra_authorize_params: Vec<(String, String)>,
}

impl Provider {
    /// Build this provider's static [`ProviderConfig`].
    pub fn config(&self) -> ProviderConfig {
        match self {
            Provider::Google => ProviderConfig {
                authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                scope: "https://www.googleapis.com/auth/calendar.readonly".to_string(),
                client_id: GOOGLE_CLIENT_ID.to_string(),
                // Google only returns a refresh_token when BOTH access_type=offline
                // and prompt=consent are present on the authorize request.
                extra_authorize_params: vec![
                    ("access_type".to_string(), "offline".to_string()),
                    ("prompt".to_string(), "consent".to_string()),
                ],
            },
            Provider::Microsoft => ProviderConfig {
                authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                    .to_string(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
                // offline_access yields a refresh_token; openid/profile identify
                // the account for the connected-email label.
                scope: "Calendars.Read offline_access openid profile".to_string(),
                client_id: MICROSOFT_CLIENT_ID.to_string(),
                extra_authorize_params: Vec::new(),
            },
        }
    }

    /// Per-provider keyring service name — the token store namespace so
    /// Google and Microsoft credentials never collide.
    pub fn keyring_service(&self) -> &'static str {
        match self {
            Provider::Google => "bluey_calendar_google",
            Provider::Microsoft => "bluey_calendar_microsoft",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_config_has_offline_consent_and_readonly_scope() {
        let cfg = Provider::Google.config();
        assert_eq!(
            cfg.authorize_url,
            "https://accounts.google.com/o/oauth2/v2/auth"
        );
        assert_eq!(cfg.token_url, "https://oauth2.googleapis.com/token");
        assert_eq!(
            cfg.scope,
            "https://www.googleapis.com/auth/calendar.readonly"
        );
        assert!(cfg
            .extra_authorize_params
            .contains(&("access_type".to_string(), "offline".to_string())));
        assert!(cfg
            .extra_authorize_params
            .contains(&("prompt".to_string(), "consent".to_string())));
    }

    #[test]
    fn microsoft_config_uses_common_tenant_and_offline_access() {
        let cfg = Provider::Microsoft.config();
        assert!(cfg.authorize_url.contains("/common/oauth2/v2.0/authorize"));
        assert!(cfg.token_url.contains("/common/oauth2/v2.0/token"));
        assert!(cfg.scope.contains("Calendars.Read"));
        assert!(cfg.scope.contains("offline_access"));
        assert!(cfg.extra_authorize_params.is_empty());
    }

    #[test]
    fn keyring_services_are_distinct_per_provider() {
        assert_eq!(Provider::Google.keyring_service(), "bluey_calendar_google");
        assert_eq!(
            Provider::Microsoft.keyring_service(),
            "bluey_calendar_microsoft"
        );
        assert_ne!(
            Provider::Google.keyring_service(),
            Provider::Microsoft.keyring_service()
        );
    }
}
