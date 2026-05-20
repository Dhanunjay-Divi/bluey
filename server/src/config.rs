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
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub deepgram_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
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
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
            deepgram_api_key: std::env::var("DEEPGRAM_API_KEY")
                .ok()
                .filter(|v| !v.is_empty()),
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

fn env_bool(name: &str) -> Option<bool> {
    std::env::var(name)
        .ok()
        .map(|value| !matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no"))
}
