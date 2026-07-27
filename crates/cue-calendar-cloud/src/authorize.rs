//! Build the OAuth authorize URL — pure, no I/O.

use url::Url;

use crate::provider::ProviderConfig;

/// Build the provider's authorize URL for a native public-client PKCE flow.
///
/// Adds the RFC 6749 / RFC 7636 required params (`client_id`, `redirect_uri`,
/// `response_type=code`, `scope`, `code_challenge`, `code_challenge_method=S256`,
/// `state`) plus any provider-specific `extra_authorize_params` (e.g. Google's
/// `access_type=offline` + `prompt=consent`).
pub fn build_authorize_url(
    cfg: &ProviderConfig,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> anyhow::Result<String> {
    let mut url = Url::parse(&cfg.authorize_url)?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &cfg.client_id);
        q.append_pair("redirect_uri", redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("scope", &cfg.scope);
        q.append_pair("code_challenge", challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", state);
        for (k, v) in &cfg.extra_authorize_params {
            q.append_pair(k, v);
        }
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use std::collections::HashMap;

    fn params(url: &str) -> HashMap<String, String> {
        Url::parse(url)
            .expect("parse url")
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[test]
    fn google_authorize_url_has_all_required_params_and_s256() {
        let cfg = Provider::Google.config();
        let url = build_authorize_url(&cfg, "http://127.0.0.1:54321", "the-challenge", "the-state")
            .expect("build url");

        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        let p = params(&url);
        assert_eq!(
            p.get("client_id").map(String::as_str),
            Some(cfg.client_id.as_str())
        );
        assert_eq!(
            p.get("redirect_uri").map(String::as_str),
            Some("http://127.0.0.1:54321")
        );
        assert_eq!(p.get("response_type").map(String::as_str), Some("code"));
        let scope = p.get("scope").expect("scope");
        assert!(scope.contains("https://www.googleapis.com/auth/calendar.readonly"));
        assert!(scope.contains("openid"));
        assert!(scope.contains("email"));
        assert_eq!(
            p.get("code_challenge").map(String::as_str),
            Some("the-challenge")
        );
        assert_eq!(
            p.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(p.get("state").map(String::as_str), Some("the-state"));
        // Provider-specific extras.
        assert_eq!(p.get("access_type").map(String::as_str), Some("offline"));
        assert_eq!(p.get("prompt").map(String::as_str), Some("consent"));
    }

    #[test]
    fn microsoft_authorize_url_has_required_params_and_no_google_extras() {
        let cfg = Provider::Microsoft.config();
        let url = build_authorize_url(&cfg, "http://127.0.0.1:6060", "chal", "st").expect("build");
        let p = params(&url);
        assert_eq!(p.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            p.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(p.get("scope").unwrap().contains("Calendars.Read"));
        assert!(!p.contains_key("access_type"));
        assert_eq!(p.get("prompt").map(String::as_str), Some("select_account"));
    }

    #[test]
    fn scope_and_redirect_are_percent_encoded() {
        let cfg = Provider::Google.config();
        let url = build_authorize_url(&cfg, "http://127.0.0.1:1", "c", "s").expect("build");
        // The raw URL string must percent-encode the scope's `/` and `:` so it
        // is a valid query value; the parsed value round-trips to the original.
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com"));
    }
}
