//! Provider configuration — pure data, no I/O.
//!
//! Endpoints, scopes, and the PUBLIC client id for each supported OAuth
//! calendar provider. Client ids are public identifiers (there is NO client
//! secret in the native public-client flow). Every binary first checks its
//! runtime environment, then falls back to the id baked into the build. This
//! lets operators configure an already-built release without putting a client
//! secret on the desktop or rebuilding Bluey.

use anyhow::{bail, Result};

const GOOGLE_PLACEHOLDER: &str = "PLACEHOLDER_GOOGLE_CLIENT_ID";
const MICROSOFT_PLACEHOLDER: &str = "PLACEHOLDER_MICROSOFT_CLIENT_ID";

/// Compile-time client-id fallbacks. A release must replace these placeholders
/// through the matching build environment variables.
const BUILT_GOOGLE_CLIENT_ID: &str = match option_env!("BLUEY_GOOGLE_CLIENT_ID") {
    Some(id) => id,
    None => GOOGLE_PLACEHOLDER,
};
const BUILT_MICROSOFT_CLIENT_ID: &str = match option_env!("BLUEY_MICROSOFT_CLIENT_ID") {
    Some(id) => id,
    None => MICROSOFT_PLACEHOLDER,
};

fn resolve_client_id(runtime_value: Option<String>, built_value: &str) -> String {
    runtime_value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| built_value.trim().to_string())
}

fn configured_client_id(runtime_key: &str, built_value: &str) -> String {
    resolve_client_id(std::env::var(runtime_key).ok(), built_value)
}

fn looks_like_microsoft_application_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().any(|byte| !matches!(byte, b'0' | b'-'))
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

/// The supported cloud calendar providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Google,
    Microsoft,
}

/// Static configuration for one provider's OAuth + calendar endpoints.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Human-readable provider name used in actionable errors.
    pub display_name: &'static str,
    /// Runtime/build environment variable that supplies the public client id.
    pub client_id_env: &'static str,
    /// Authorization endpoint (where the browser is sent for consent).
    pub authorize_url: String,
    /// Token endpoint (code exchange + refresh).
    pub token_url: String,
    /// Requested OAuth scope string (space-delimited).
    pub scope: String,
    /// PUBLIC OAuth client id (not a secret).
    pub client_id: String,
    /// Host used in the native loopback redirect. Microsoft public clients are
    /// registered with `http://localhost`; Google desktop clients use the
    /// literal loopback address.
    pub loopback_host: String,
    /// Extra params appended to the authorize URL (e.g. Google's
    /// `access_type=offline` + `prompt=consent` to guarantee a refresh token).
    pub extra_authorize_params: Vec<(String, String)>,
}

impl ProviderConfig {
    /// Fail before opening a browser when neither runtime nor build-time
    /// configuration supplies a registered public client id. Previously Bluey
    /// sent users to an invalid consent page and only explained the problem
    /// after a long timeout.
    pub fn validate(&self) -> Result<()> {
        let client_id = self.client_id.trim();
        if client_id.is_empty()
            || client_id == GOOGLE_PLACEHOLDER
            || client_id == MICROSOFT_PLACEHOLDER
            || client_id.starts_with("PLACEHOLDER_")
        {
            bail!(
                "{} calendar OAuth is not configured; install a calendar-enabled \
                 Bluey release or set {} in the Bluey daemon environment and restart Bluey",
                self.display_name,
                self.client_id_env
            );
        }
        let correctly_shaped = match self.client_id_env {
            "BLUEY_GOOGLE_CLIENT_ID" => {
                client_id.ends_with(".apps.googleusercontent.com")
                    && client_id.len() > ".apps.googleusercontent.com".len()
            }
            "BLUEY_MICROSOFT_CLIENT_ID" => looks_like_microsoft_application_id(client_id),
            _ => false,
        };
        if !correctly_shaped {
            bail!(
                "{} calendar client ID is malformed; set {} in the Bluey daemon \
                 environment to the registered native/public application ID and restart Bluey",
                self.display_name,
                self.client_id_env
            );
        }
        Ok(())
    }
}

impl Provider {
    /// Build this provider's static [`ProviderConfig`].
    pub fn config(&self) -> ProviderConfig {
        match self {
            Provider::Google => ProviderConfig {
                display_name: "Google",
                client_id_env: "BLUEY_GOOGLE_CLIENT_ID",
                authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
                token_url: "https://oauth2.googleapis.com/token".to_string(),
                // `openid email` is needed for the connected-account label;
                // calendar data remains strictly read-only.
                scope: "https://www.googleapis.com/auth/calendar.readonly openid email".to_string(),
                client_id: configured_client_id("BLUEY_GOOGLE_CLIENT_ID", BUILT_GOOGLE_CLIENT_ID),
                loopback_host: "127.0.0.1".to_string(),
                // Google only returns a refresh_token when BOTH access_type=offline
                // and prompt=consent are present on the authorize request.
                extra_authorize_params: vec![
                    ("access_type".to_string(), "offline".to_string()),
                    ("prompt".to_string(), "consent".to_string()),
                ],
            },
            Provider::Microsoft => ProviderConfig {
                display_name: "Microsoft",
                client_id_env: "BLUEY_MICROSOFT_CLIENT_ID",
                authorize_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize"
                    .to_string(),
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
                // offline_access yields a refresh_token; openid/profile identify
                // the account for the connected-email label.
                // Graph `/me` requires User.Read for the connected-account
                // label. Calendar access itself remains read-only.
                scope: "Calendars.Read User.Read offline_access openid profile".to_string(),
                client_id: configured_client_id(
                    "BLUEY_MICROSOFT_CLIENT_ID",
                    BUILT_MICROSOFT_CLIENT_ID,
                ),
                loopback_host: "localhost".to_string(),
                extra_authorize_params: vec![("prompt".to_string(), "select_account".to_string())],
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

    const RUNTIME_ENV_CHILD: &str = "BLUEY_PROVIDER_CONFIG_RUNTIME_TEST_CHILD";
    const RUNTIME_GOOGLE_CLIENT_ID: &str = "runtime-test-client.apps.googleusercontent.com";

    #[test]
    fn google_config_has_offline_consent_and_readonly_scope() {
        let cfg = Provider::Google.config();
        assert_eq!(
            cfg.authorize_url,
            "https://accounts.google.com/o/oauth2/v2/auth"
        );
        assert_eq!(cfg.token_url, "https://oauth2.googleapis.com/token");
        assert!(cfg
            .scope
            .contains("https://www.googleapis.com/auth/calendar.readonly"));
        assert!(cfg.scope.contains("openid"));
        assert!(cfg.scope.contains("email"));
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
        assert!(cfg.scope.contains("User.Read"));
        assert!(cfg.scope.contains("offline_access"));
        assert!(cfg
            .extra_authorize_params
            .contains(&("prompt".to_string(), "select_account".to_string())));
        assert_eq!(cfg.loopback_host, "localhost");
    }

    #[test]
    fn placeholder_client_id_fails_before_browser_flow() {
        let mut cfg = Provider::Google.config();
        cfg.client_id = GOOGLE_PLACEHOLDER.to_string();
        let error = cfg.validate().expect_err("placeholder must fail");
        let message = error.to_string();
        assert!(message.contains("Google calendar OAuth is not configured"));
        assert!(message.contains("BLUEY_GOOGLE_CLIENT_ID"));
        assert!(message.contains("daemon environment"));
        assert!(message.contains("restart Bluey"));
    }

    #[test]
    fn non_empty_runtime_client_id_overrides_built_release_value() {
        let resolved = resolve_client_id(
            Some(" runtime.apps.googleusercontent.com ".to_string()),
            "built.apps.googleusercontent.com",
        );
        assert_eq!(resolved, "runtime.apps.googleusercontent.com");
    }

    #[test]
    fn provider_config_reads_the_daemon_runtime_environment() {
        if std::env::var_os(RUNTIME_ENV_CHILD).is_some() {
            let config = Provider::Google.config();
            assert_eq!(config.client_id, RUNTIME_GOOGLE_CLIENT_ID);
            config
                .validate()
                .expect("runtime public client id should be accepted");
            return;
        }

        // Environment mutation is process-global, so exercise the real
        // `std::env` path in a child test process instead of racing sibling
        // tests in this process.
        let output = std::process::Command::new(
            std::env::current_exe().expect("locate current test binary"),
        )
        .args([
            "--exact",
            "provider::tests::provider_config_reads_the_daemon_runtime_environment",
            "--nocapture",
        ])
        .env(RUNTIME_ENV_CHILD, "1")
        .env("BLUEY_GOOGLE_CLIENT_ID", RUNTIME_GOOGLE_CLIENT_ID)
        .output()
        .expect("run provider config child test");

        assert!(
            output.status.success(),
            "runtime environment child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn empty_runtime_client_id_preserves_built_release_value() {
        assert_eq!(
            resolve_client_id(Some(" \t ".to_string()), " built-client-id "),
            "built-client-id"
        );
        assert_eq!(
            resolve_client_id(None, " built-client-id "),
            "built-client-id"
        );
    }

    #[test]
    fn malformed_runtime_override_is_not_silently_replaced_by_built_value() {
        let mut cfg = Provider::Google.config();
        cfg.client_id = resolve_client_id(
            Some("not-a-google-client".to_string()),
            "built.apps.googleusercontent.com",
        );

        let error = cfg
            .validate()
            .expect_err("an explicit malformed override must be actionable");
        assert!(error.to_string().contains("BLUEY_GOOGLE_CLIENT_ID"));
    }

    #[test]
    fn configured_public_client_needs_no_secret() {
        let mut cfg = Provider::Microsoft.config();
        cfg.client_id = "12345678-1234-1234-1234-123456789abc".to_string();
        cfg.validate().expect("public client id is sufficient");
    }

    #[test]
    fn malformed_public_client_ids_fail_before_browser_flow() {
        let mut google = Provider::Google.config();
        google.client_id = "not-a-google-client".to_string();
        assert!(google.validate().is_err());

        let mut microsoft = Provider::Microsoft.config();
        microsoft.client_id = "not-a-guid".to_string();
        assert!(microsoft.validate().is_err());

        microsoft.client_id = "00000000-0000-0000-0000-000000000000".to_string();
        assert!(microsoft.validate().is_err());
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
