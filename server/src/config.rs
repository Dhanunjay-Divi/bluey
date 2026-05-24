//! Server-side configuration. Read from environment variables.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP port. Default 8080.
    pub port: u16,
    /// SQLite DB path. Default /opt/bluey-api/bluey.db (or ./bluey-dev.db in dev).
    pub db_path: PathBuf,
    /// JWT signing secret. REQUIRED. Server refuses to boot without it.
    pub jwt_secret: String,
    /// Public URL used in templated install scripts and email links.
    pub public_url: String,
    /// Stripe secret key (test or live). Optional in dev; required for live billing.
    pub stripe_secret_key: Option<String>,
    /// Stripe webhook signing secret. Optional in dev.
    pub stripe_webhook_secret: Option<String>,
    /// Upstream provider keys held by Bluey. The managed Auto Router endpoint
    /// uses these to dispatch LLM / embedding / vision / STT calls.
    pub upstream: UpstreamKeys,
    /// Transactional email transport. Optional in dev; when unset the server
    /// logs local verification/reset URLs instead of sending mail.
    pub smtp: Option<SmtpConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct UpstreamKeys {
    /// Single key or comma-separated, provider-approved key pool.
    pub openai_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub anthropic_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub deepgram_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
}

impl UpstreamKeys {
    pub fn openai_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.openai_api_key.as_deref(), shard_key)
    }

    pub fn anthropic_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.anthropic_api_key.as_deref(), shard_key)
    }

    pub fn deepgram_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.deepgram_api_key.as_deref(), shard_key)
    }
}

#[derive(Debug, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: String,
    pub starttls: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let port = std::env::var("BLUEY_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        let db_path = std::env::var("BLUEY_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./bluey-dev.db"));

        let jwt_secret = std::env::var("BLUEY_JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("BLUEY_JWT_SECRET is required"))?;
        if jwt_secret.len() < 32 {
            anyhow::bail!("BLUEY_JWT_SECRET must be at least 32 chars");
        }

        let public_url = std::env::var("BLUEY_PUBLIC_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());

        let stripe_secret_key = std::env::var("STRIPE_SECRET_KEY")
            .ok()
            .filter(|v| !v.is_empty());
        let stripe_webhook_secret = std::env::var("STRIPE_WEBHOOK_SECRET")
            .ok()
            .filter(|v| !v.is_empty());

        let upstream = UpstreamKeys {
            openai_api_key: env_any(&["OPENAI_API_KEYS", "OPENAI_API_KEY"]),
            anthropic_api_key: env_any(&["ANTHROPIC_API_KEYS", "ANTHROPIC_API_KEY"]),
            deepgram_api_key: env_any(&["DEEPGRAM_API_KEYS", "DEEPGRAM_API_KEY"]),
            ollama_base_url: std::env::var("OLLAMA_BASE_URL")
                .ok()
                .filter(|v| !v.is_empty()),
        };

        let smtp = std::env::var("BLUEY_SMTP_HOST")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|host| SmtpConfig {
                host,
                port: std::env::var("BLUEY_SMTP_PORT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(587),
                username: std::env::var("BLUEY_SMTP_USERNAME")
                    .ok()
                    .filter(|v| !v.is_empty()),
                password: std::env::var("BLUEY_SMTP_PASSWORD")
                    .ok()
                    .filter(|v| !v.is_empty()),
                from: std::env::var("BLUEY_SMTP_FROM")
                    .unwrap_or_else(|_| "Bluey <no-reply@bluey.sh>".to_string()),
                starttls: env_bool("BLUEY_SMTP_STARTTLS").unwrap_or(true),
            });

        Ok(Self {
            port,
            db_path,
            jwt_secret,
            public_url,
            stripe_secret_key,
            stripe_webhook_secret,
            upstream,
            smtp,
        })
    }
}

fn env_any(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()))
}

fn env_bool(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        )
    })
}

fn select_key_from_pool<'a>(raw: Option<&'a str>, shard_key: &str) -> Option<&'a str> {
    let raw = raw?;
    let keys = raw
        .split(',')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        return None;
    }
    let idx = (stable_hash(shard_key) as usize) % keys.len();
    Some(keys[idx])
}

fn stable_hash(input: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_pool_selects_single_key() {
        let keys = UpstreamKeys {
            openai_api_key: Some("sk-one".into()),
            ..Default::default()
        };
        assert_eq!(keys.openai_key("anything"), Some("sk-one"));
    }

    #[test]
    fn key_pool_ignores_empty_entries() {
        let keys = UpstreamKeys {
            anthropic_api_key: Some(" , ak-one, , ak-two ".into()),
            ..Default::default()
        };
        let selected = keys.anthropic_key("stable-shard").unwrap();
        assert!(selected == "ak-one" || selected == "ak-two");
    }

    #[test]
    fn key_pool_spreads_across_keys() {
        let keys = UpstreamKeys {
            deepgram_api_key: Some("dg-a,dg-b,dg-c".into()),
            ..Default::default()
        };
        let mut seen = std::collections::BTreeSet::new();
        for idx in 0..64 {
            seen.insert(keys.deepgram_key(&format!("request-{idx}")).unwrap());
        }
        assert!(seen.len() >= 2, "expected pool to use more than one key");
    }
}
