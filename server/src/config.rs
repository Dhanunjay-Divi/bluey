//! Server-side configuration. Read from environment variables.

use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP port. Default 8080.
    pub port: u16,
    /// SQLite DB path. Default /opt/bluey-api/bluey.db (or ./bluey-dev.db in dev).
    pub db_path: PathBuf,
    /// Runtime database backend selector. Today only SQLite is implemented.
    pub db_backend: ServerDbBackend,
    /// Postgres connection string used when BLUEY_SERVER_DB_BACKEND=postgres.
    pub database_url: Option<String>,
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
    /// Optional rolling upstream-spend guardrail for live testing. This is a
    /// Bluey-side safety cap; provider dashboards should still have their own
    /// hard billing limits where available.
    pub upstream_spend_guard: Option<UpstreamSpendGuard>,
    /// Transactional email transport. Optional in dev; when unset the server
    /// logs local verification/reset URLs instead of sending mail.
    pub smtp: Option<SmtpConfig>,
    /// Operator/admin accounts. Matching signup emails are created as admins;
    /// matching existing accounts are promoted on next login.
    pub admin_emails: Vec<String>,
    /// Trial and signup abuse controls.
    pub trial_abuse: TrialAbuseConfig,
    /// Cloudflare Turnstile public site key exposed to the browser when set.
    pub turnstile_site_key: Option<String>,
    /// Cloudflare Turnstile secret. When set, signup/start requires a valid token.
    pub turnstile_secret_key: Option<String>,
    /// Fail signup closed when Turnstile is required but not fully configured.
    pub require_turnstile: bool,
    /// Optional S3-compatible object storage for synced document/image bytes.
    pub object_storage: Option<ObjectStorageConfig>,
    /// Optional S3-compatible object storage for diagnostic log chunks.
    pub log_storage: Option<ObjectStorageConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServerDbBackend {
    #[default]
    Sqlite,
    Postgres,
}

impl ServerDbBackend {
    fn from_env_value(value: &str) -> anyhow::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "sqlite" => Ok(Self::Sqlite),
            "postgres" | "postgresql" => Ok(Self::Postgres),
            other => {
                anyhow::bail!("BLUEY_SERVER_DB_BACKEND must be sqlite or postgres, got {other:?}")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingProvider {
    Stripe,
    Square,
}

impl BillingProvider {
    fn from_env_value(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "stripe" => Some(Self::Stripe),
            "square" => Some(Self::Square),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquareEnvironment {
    #[default]
    Sandbox,
    Production,
}

impl SquareEnvironment {
    pub fn api_base_url(self) -> &'static str {
        match self {
            Self::Sandbox => "https://connect.squareupsandbox.com",
            Self::Production => "https://connect.squareup.com",
        }
    }

    fn env_prefix(self) -> &'static str {
        match self {
            Self::Sandbox => "SQUARE_SANDBOX",
            Self::Production => "SQUARE_PRODUCTION",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SquareConfig {
    pub environment: SquareEnvironment,
    pub application_id: Option<String>,
    pub access_token: Option<String>,
    pub location_id: Option<String>,
    pub webhook_signature_key: Option<String>,
    pub webhook_notification_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SquareWebhookSignatureConfig {
    pub environment: SquareEnvironment,
    pub webhook_signature_key: String,
    pub webhook_notification_url: String,
}

impl SquareConfig {
    pub fn is_checkout_ready(&self) -> bool {
        self.access_token.is_some() && self.location_id.is_some()
    }

    pub fn is_webhook_ready(&self) -> bool {
        self.webhook_signature_key.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpstreamKeys {
    /// Single key or comma-separated, provider-approved key pool.
    pub openai_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub anthropic_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub gemini_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub deepseek_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub zai_api_key: Option<String>,
    /// Single key or comma-separated, provider-approved key pool.
    pub deepgram_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Mandatory admission boundary for managed paid provider dispatch.
///
/// `None` means no positive limit was configured; it never means unlimited.
/// Every managed paid route must fail closed before provider I/O in that state.
pub struct UpstreamSpendGuard {
    pub limit_cents: i64,
    pub window_hours: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrialAbuseConfig {
    pub max_trials_per_email: i64,
    pub max_trials_per_email_domain_per_day: i64,
    pub max_trials_per_device: i64,
    pub max_trials_per_device_per_30_days: i64,
    pub max_trials_per_ip_per_day: i64,
    pub max_trials_per_ip_user_agent_per_day: i64,
}

#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    pub endpoint_url: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub key_prefix: String,
    pub retention_days: i64,
    pub max_object_bytes: usize,
}

impl Default for TrialAbuseConfig {
    fn default() -> Self {
        Self {
            max_trials_per_email: 1,
            max_trials_per_email_domain_per_day: 25,
            max_trials_per_device: 6,
            max_trials_per_device_per_30_days: 1,
            max_trials_per_ip_per_day: 3,
            max_trials_per_ip_user_agent_per_day: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamKeyCandidate {
    pub secret: String,
    pub fingerprint: String,
}

impl UpstreamKeys {
    pub fn openai_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.openai_api_key.as_deref(), shard_key)
    }

    pub fn anthropic_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.anthropic_api_key.as_deref(), shard_key)
    }

    pub fn gemini_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.gemini_api_key.as_deref(), shard_key)
    }

    pub fn deepseek_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.deepseek_api_key.as_deref(), shard_key)
    }

    pub fn zai_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.zai_api_key.as_deref(), shard_key)
    }

    pub fn deepgram_key(&self, shard_key: &str) -> Option<&str> {
        select_key_from_pool(self.deepgram_api_key.as_deref(), shard_key)
    }

    pub fn key_candidates(&self, provider: &str, shard_key: &str) -> Vec<UpstreamKeyCandidate> {
        let raw = match provider {
            "openai" => self.openai_api_key.as_deref(),
            "anthropic" => self.anthropic_api_key.as_deref(),
            "gemini" => self.gemini_api_key.as_deref(),
            "deepseek" => self.deepseek_api_key.as_deref(),
            "zai" => self.zai_api_key.as_deref(),
            "deepgram" => self.deepgram_api_key.as_deref(),
            _ => None,
        };
        key_candidates_from_pool(raw, shard_key)
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
        let db_backend = ServerDbBackend::from_env_value(
            &std::env::var("BLUEY_SERVER_DB_BACKEND").unwrap_or_else(|_| "sqlite".to_string()),
        )?;
        let database_url = std::env::var("BLUEY_DATABASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty());

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
            gemini_api_key: env_any(&[
                "GEMINI_API_KEYS",
                "GEMINI_API_KEY",
                "GOOGLE_API_KEYS",
                "GOOGLE_API_KEY",
            ]),
            deepseek_api_key: env_any(&["DEEPSEEK_API_KEYS", "DEEPSEEK_API_KEY"]),
            zai_api_key: env_any(&[
                "ZAI_API_KEYS",
                "ZAI_API_KEY",
                "ZHIPU_API_KEYS",
                "ZHIPU_API_KEY",
            ]),
            deepgram_api_key: env_any(&["DEEPGRAM_API_KEYS", "DEEPGRAM_API_KEY"]),
            ollama_base_url: std::env::var("OLLAMA_BASE_URL")
                .ok()
                .filter(|v| !v.is_empty()),
        };
        let upstream_spend_guard = upstream_spend_guard_from_env();

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
        let admin_emails = parse_email_list(std::env::var("BLUEY_ADMIN_EMAILS").ok());
        let trial_abuse = TrialAbuseConfig {
            max_trials_per_email: env_positive_i64("BLUEY_TRIAL_MAX_PER_EMAIL").unwrap_or(1),
            max_trials_per_email_domain_per_day: env_positive_i64(
                "BLUEY_TRIAL_MAX_PER_EMAIL_DOMAIN_PER_DAY",
            )
            .unwrap_or(25),
            max_trials_per_device: env_positive_i64("BLUEY_TRIAL_MAX_PER_DEVICE").unwrap_or(6),
            max_trials_per_device_per_30_days: env_positive_i64(
                "BLUEY_TRIAL_MAX_PER_DEVICE_PER_30_DAYS",
            )
            .unwrap_or(1),
            max_trials_per_ip_per_day: env_positive_i64("BLUEY_TRIAL_MAX_PER_IP_PER_DAY")
                .unwrap_or(3),
            max_trials_per_ip_user_agent_per_day: env_positive_i64(
                "BLUEY_TRIAL_MAX_PER_IP_USER_AGENT_PER_DAY",
            )
            .unwrap_or(5),
        };
        let turnstile_site_key = env_any(&["BLUEY_TURNSTILE_SITE_KEY", "TURNSTILE_SITE_KEY"]);
        let turnstile_secret_key = env_any(&["BLUEY_TURNSTILE_SECRET_KEY", "TURNSTILE_SECRET_KEY"]);
        let require_turnstile = env_bool("BLUEY_REQUIRE_TURNSTILE").unwrap_or(false);
        let object_storage = object_storage_from_env();
        let log_storage = log_storage_from_env();

        Ok(Self {
            port,
            db_path,
            db_backend,
            database_url,
            jwt_secret,
            public_url,
            stripe_secret_key,
            stripe_webhook_secret,
            upstream,
            upstream_spend_guard,
            smtp,
            admin_emails,
            trial_abuse,
            turnstile_site_key,
            turnstile_secret_key,
            require_turnstile,
            object_storage,
            log_storage,
        })
    }

    pub fn is_admin_email(&self, email: &str) -> bool {
        let normalized = normalize_email(email);
        self.admin_emails.iter().any(|admin| admin == &normalized)
    }

    pub fn billing_provider(&self) -> BillingProvider {
        if let Some(provider) = std::env::var("BLUEY_BILLING_PROVIDER")
            .ok()
            .and_then(|v| BillingProvider::from_env_value(&v))
        {
            return provider;
        }

        let square = self.square_config();
        if square.is_checkout_ready() {
            return BillingProvider::Square;
        }

        BillingProvider::Stripe
    }

    pub fn square_config(&self) -> SquareConfig {
        let environment = square_environment_from_env();
        let prefix = environment.env_prefix();

        SquareConfig {
            environment,
            application_id: env_any(&[
                "SQUARE_APPLICATION_ID",
                &format!("{prefix}_APPLICATION_ID"),
            ]),
            access_token: env_any(&["SQUARE_ACCESS_TOKEN", &format!("{prefix}_ACCESS_TOKEN")]),
            location_id: env_any(&["SQUARE_LOCATION_ID", &format!("{prefix}_LOCATION_ID")]),
            webhook_signature_key: env_any(&[
                "SQUARE_WEBHOOK_SIGNATURE_KEY",
                &format!("{prefix}_WEBHOOK_SIGNATURE_KEY"),
            ]),
            webhook_notification_url: std::env::var("SQUARE_WEBHOOK_NOTIFICATION_URL")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| Some(format!("{}/billing/square/webhook", self.public_url))),
        }
    }

    pub fn square_webhook_signature_configs(&self) -> Vec<SquareWebhookSignatureConfig> {
        let notification_url = std::env::var("SQUARE_WEBHOOK_NOTIFICATION_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("{}/billing/square/webhook", self.public_url));
        let configured = square_environment_from_env();
        let ordered = match configured {
            SquareEnvironment::Sandbox => {
                [SquareEnvironment::Sandbox, SquareEnvironment::Production]
            }
            SquareEnvironment::Production => {
                [SquareEnvironment::Production, SquareEnvironment::Sandbox]
            }
        };
        let generic_key = std::env::var("SQUARE_WEBHOOK_SIGNATURE_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let mut configs = Vec::new();
        for environment in ordered {
            let prefix = environment.env_prefix();
            let environment_key = std::env::var(format!("{prefix}_WEBHOOK_SIGNATURE_KEY"))
                .ok()
                .filter(|v| !v.trim().is_empty());
            let key = environment_key.or_else(|| {
                if environment == configured {
                    generic_key.clone()
                } else {
                    None
                }
            });
            if let Some(webhook_signature_key) = key {
                if configs.iter().any(|config: &SquareWebhookSignatureConfig| {
                    config.webhook_signature_key == webhook_signature_key
                }) {
                    continue;
                }
                configs.push(SquareWebhookSignatureConfig {
                    environment,
                    webhook_signature_key,
                    webhook_notification_url: notification_url.clone(),
                });
            }
        }
        configs
    }
}

fn object_storage_from_env() -> Option<ObjectStorageConfig> {
    let endpoint_url = env_any(&[
        "BLUEY_OBJECT_ENDPOINT_URL",
        "BLUEY_R2_ENDPOINT_URL",
        "AWS_ENDPOINT_URL_S3",
    ])?;
    let bucket = env_any(&["BLUEY_OBJECT_BUCKET", "BLUEY_R2_BUCKET", "AWS_S3_BUCKET"])?;
    let access_key_id = env_any(&[
        "BLUEY_OBJECT_ACCESS_KEY_ID",
        "BLUEY_R2_ACCESS_KEY_ID",
        "AWS_ACCESS_KEY_ID",
    ])?;
    let secret_access_key = env_any(&[
        "BLUEY_OBJECT_SECRET_ACCESS_KEY",
        "BLUEY_R2_SECRET_ACCESS_KEY",
        "AWS_SECRET_ACCESS_KEY",
    ])?;
    let region = std::env::var("BLUEY_OBJECT_REGION")
        .or_else(|_| std::env::var("BLUEY_R2_REGION"))
        .or_else(|_| std::env::var("AWS_REGION"))
        .unwrap_or_else(|_| "auto".to_string());
    let key_prefix = std::env::var("BLUEY_OBJECT_KEY_PREFIX")
        .unwrap_or_else(|_| "bluey-cloud".to_string())
        .trim_matches('/')
        .to_string();
    let retention_days = env_positive_i64("BLUEY_OBJECT_RETENTION_DAYS").unwrap_or(365);
    let max_object_bytes = std::env::var("BLUEY_OBJECT_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(25 * 1024 * 1024);

    Some(ObjectStorageConfig {
        endpoint_url,
        bucket,
        access_key_id,
        secret_access_key,
        region,
        key_prefix,
        retention_days,
        max_object_bytes,
    })
}

fn log_storage_from_env() -> Option<ObjectStorageConfig> {
    let storage = std::env::var("BLUEY_LOG_STORAGE")
        .unwrap_or_else(|_| "".to_string())
        .trim()
        .to_ascii_lowercase();
    if !storage.is_empty() && storage != "r2" && storage != "s3" {
        return None;
    }

    let endpoint_url = env_any(&[
        "BLUEY_LOG_R2_ENDPOINT_URL",
        "BLUEY_LOG_R2_ENDPOINT",
        "BLUEY_OBJECT_ENDPOINT_URL",
        "BLUEY_R2_ENDPOINT_URL",
        "AWS_ENDPOINT_URL_S3",
    ])?;
    let bucket = env_any(&[
        "BLUEY_LOG_R2_BUCKET",
        "BLUEY_OBJECT_BUCKET",
        "BLUEY_R2_BUCKET",
        "AWS_S3_BUCKET",
    ])?;
    let access_key_id = env_any(&[
        "BLUEY_LOG_R2_ACCESS_KEY_ID",
        "BLUEY_OBJECT_ACCESS_KEY_ID",
        "BLUEY_R2_ACCESS_KEY_ID",
        "AWS_ACCESS_KEY_ID",
    ])?;
    let secret_access_key = env_any(&[
        "BLUEY_LOG_R2_SECRET_ACCESS_KEY",
        "BLUEY_OBJECT_SECRET_ACCESS_KEY",
        "BLUEY_R2_SECRET_ACCESS_KEY",
        "AWS_SECRET_ACCESS_KEY",
    ])?;
    let region = std::env::var("BLUEY_LOG_R2_REGION")
        .or_else(|_| std::env::var("BLUEY_OBJECT_REGION"))
        .or_else(|_| std::env::var("BLUEY_R2_REGION"))
        .or_else(|_| std::env::var("AWS_REGION"))
        .unwrap_or_else(|_| "auto".to_string());
    let raw_prefix =
        std::env::var("BLUEY_LOG_STORAGE_PREFIX").unwrap_or_else(|_| "prod".to_string());
    let key_prefix = log_storage_key_prefix(&raw_prefix);
    let retention_days = env_positive_i64("BLUEY_UPLOAD_LOG_RETENTION_DAYS")
        .or_else(|| env_positive_i64("BLUEY_LOG_RETENTION_DAYS"))
        .unwrap_or(180)
        .clamp(1, 180);
    let max_object_bytes = std::env::var("BLUEY_UPLOAD_LOG_MAX_BYTES")
        .or_else(|_| std::env::var("BLUEY_LOG_OBJECT_MAX_BYTES"))
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(32 * 1024 * 1024);

    Some(ObjectStorageConfig {
        endpoint_url,
        bucket,
        access_key_id,
        secret_access_key,
        region,
        key_prefix,
        retention_days,
        max_object_bytes,
    })
}

fn log_storage_key_prefix(raw: &str) -> String {
    let prefix = raw.trim_matches('/').trim();
    if prefix.is_empty() {
        return "logs".to_string();
    }
    if prefix == "logs" || prefix.ends_with("/logs") || prefix.contains("/logs/") {
        prefix.to_string()
    } else {
        format!("{prefix}/logs")
    }
}

fn upstream_spend_guard_from_env() -> Option<UpstreamSpendGuard> {
    // Unset, invalid, and non-positive values deliberately produce no guard.
    // Paid dispatchers interpret None as denied, not as an unlimited budget.
    let limit_cents = std::env::var("BLUEY_UPSTREAM_SPEND_LIMIT_CENTS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)?;
    let window_hours = std::env::var("BLUEY_UPSTREAM_SPEND_WINDOW_HOURS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(24);
    Some(UpstreamSpendGuard {
        limit_cents,
        window_hours,
    })
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

fn env_positive_i64(name: &str) -> Option<i64> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn parse_email_list(raw: Option<String>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(normalize_email)
        .filter(|email| email.contains('@'))
        .collect()
}

fn square_environment_from_env() -> SquareEnvironment {
    let value = std::env::var("SQUARE_ENVIRONMENT")
        .or_else(|_| std::env::var("BLUEY_ENVIRONMENT"))
        .unwrap_or_else(|_| "sandbox".to_string());
    match value.trim().to_ascii_lowercase().as_str() {
        "prod" | "production" | "live" => SquareEnvironment::Production,
        _ => SquareEnvironment::Sandbox,
    }
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

fn key_candidates_from_pool(raw: Option<&str>, shard_key: &str) -> Vec<UpstreamKeyCandidate> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let keys = raw
        .split(',')
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>();
    if keys.is_empty() {
        return Vec::new();
    }
    // Per-request seeded shuffle. A rotation would send every request whose
    // start lands on a cooling key to the SAME next key (herd-onto-next
    // during a cooldown). A full shuffle fans displaced load across all
    // healthy keys. Deterministic per shard_key so a retry of the same
    // request_id prefers the same key first.
    shuffled_indices(keys.len(), stable_hash(shard_key))
        .into_iter()
        .map(|idx| keys[idx])
        .map(|key| UpstreamKeyCandidate {
            secret: key.to_string(),
            fingerprint: key_fingerprint(key),
        })
        .collect()
}

/// Deterministic Fisher-Yates shuffle of `0..n` seeded by `seed` (xorshift64).
/// Same seed -> same permutation. Gives each request an independent ordering
/// over the key pool without any shared/round-robin state.
fn shuffled_indices(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    if n <= 1 {
        return order;
    }
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    if state == 0 {
        state = 0xDEAD_BEEF;
    }
    for i in (1..n).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

fn key_fingerprint(key: &str) -> String {
    let digest = Sha256::digest(key.as_bytes());
    hex::encode(&digest[..8])
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

    #[test]
    fn gemini_key_pool_uses_same_sharding() {
        let keys = UpstreamKeys {
            gemini_api_key: Some("gm-a,gm-b,gm-c".into()),
            ..Default::default()
        };
        let selected = keys.gemini_key("vision-request").unwrap();
        assert!(["gm-a", "gm-b", "gm-c"].contains(&selected));
        assert_eq!(keys.key_candidates("gemini", "vision-request").len(), 3);
    }

    #[test]
    fn openai_compatible_key_pools_are_supported() {
        let keys = UpstreamKeys {
            deepseek_api_key: Some("ds-a,ds-b".into()),
            zai_api_key: Some("zai-a,zai-b".into()),
            ..Default::default()
        };
        assert!(["ds-a", "ds-b"].contains(&keys.deepseek_key("chat").unwrap()));
        assert!(["zai-a", "zai-b"].contains(&keys.zai_key("chat").unwrap()));
        assert_eq!(keys.key_candidates("deepseek", "chat").len(), 2);
        assert_eq!(keys.key_candidates("zai", "chat").len(), 2);
    }

    #[test]
    fn key_candidates_rotate_without_exposing_raw_key_as_fingerprint() {
        let keys = UpstreamKeys {
            openai_api_key: Some("sk-a,sk-b,sk-c".into()),
            ..Default::default()
        };
        let candidates = keys.key_candidates("openai", "request-1");
        assert_eq!(candidates.len(), 3);
        let secrets = candidates
            .iter()
            .map(|candidate| candidate.secret.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(secrets.len(), 3);
        assert!(candidates
            .iter()
            .all(|candidate| candidate.fingerprint.len() == 16));
        assert!(candidates
            .iter()
            .all(|candidate| candidate.fingerprint != candidate.secret));
    }

    #[test]
    fn shuffled_indices_is_a_deterministic_permutation() {
        assert_eq!(shuffled_indices(5, 42), shuffled_indices(5, 42));
        for n in [1usize, 2, 3, 8] {
            let mut sorted = shuffled_indices(n, 0xABCD);
            sorted.sort_unstable();
            assert_eq!(
                sorted,
                (0..n).collect::<Vec<_>>(),
                "n={n} not a permutation"
            );
        }
        assert_ne!(shuffled_indices(4, 1), shuffled_indices(4, 2));
    }

    #[test]
    fn key_candidates_fan_out_second_choice() {
        let keys = UpstreamKeys {
            openai_api_key: Some("sk-a,sk-b,sk-c,sk-d".into()),
            ..Default::default()
        };
        let mut firsts = std::collections::BTreeSet::new();
        let mut seconds_for_a = std::collections::BTreeSet::new();
        for idx in 0..256 {
            let cands = keys.key_candidates("openai", &format!("req-{idx}"));
            assert_eq!(cands.len(), 4);
            firsts.insert(cands[0].secret.clone());
            if cands[0].secret == "sk-a" {
                seconds_for_a.insert(cands[1].secret.clone());
            }
        }
        assert!(
            firsts.len() >= 3,
            "first pick should spread across the pool"
        );
        assert!(
            seconds_for_a.len() >= 2,
            "second pick after sk-a should fan out, got {seconds_for_a:?}"
        );
    }

    #[test]
    fn square_config_uses_environment_specific_keys() {
        std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
        std::env::set_var("SQUARE_SANDBOX_ACCESS_TOKEN", "sandbox-token");
        std::env::set_var("SQUARE_SANDBOX_LOCATION_ID", "sandbox-location");
        std::env::set_var("SQUARE_PRODUCTION_ACCESS_TOKEN", "prod-token");
        std::env::set_var("SQUARE_PRODUCTION_LOCATION_ID", "prod-location");

        let cfg = Config {
            port: 0,
            db_path: PathBuf::from(":memory:"),
            db_backend: ServerDbBackend::Sqlite,
            database_url: None,
            jwt_secret: "test_secret_at_least_32_chars_long_xx".to_string(),
            public_url: "https://bluey.sh".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            upstream: UpstreamKeys::default(),
            upstream_spend_guard: None,
            smtp: None,
            admin_emails: vec![],
            trial_abuse: TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: None,
            log_storage: None,
        };

        let square = cfg.square_config();
        assert_eq!(square.environment, SquareEnvironment::Sandbox);
        assert_eq!(square.access_token.as_deref(), Some("sandbox-token"));
        assert_eq!(square.location_id.as_deref(), Some("sandbox-location"));
        assert_eq!(cfg.billing_provider(), BillingProvider::Square);

        std::env::set_var("SQUARE_ENVIRONMENT", "production");
        let square = cfg.square_config();
        assert_eq!(square.environment, SquareEnvironment::Production);
        assert_eq!(square.access_token.as_deref(), Some("prod-token"));
        assert_eq!(square.location_id.as_deref(), Some("prod-location"));

        std::env::set_var(
            "SQUARE_WEBHOOK_NOTIFICATION_URL",
            "https://bluey.sh/billing/square/webhook",
        );
        std::env::set_var("SQUARE_WEBHOOK_SIGNATURE_KEY", "generic-whsec");
        std::env::set_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY", "sandbox-whsec");
        std::env::set_var("SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY", "prod-whsec");
        std::env::set_var("SQUARE_ENVIRONMENT", "sandbox");
        let webhook_configs = cfg.square_webhook_signature_configs();
        assert_eq!(webhook_configs.len(), 2);
        assert_eq!(webhook_configs[0].environment, SquareEnvironment::Sandbox);
        assert_eq!(webhook_configs[0].webhook_signature_key, "sandbox-whsec");
        assert_eq!(
            webhook_configs[1].environment,
            SquareEnvironment::Production
        );
        assert_eq!(webhook_configs[1].webhook_signature_key, "prod-whsec");

        std::env::remove_var("SQUARE_ENVIRONMENT");
        std::env::remove_var("SQUARE_SANDBOX_ACCESS_TOKEN");
        std::env::remove_var("SQUARE_SANDBOX_LOCATION_ID");
        std::env::remove_var("SQUARE_PRODUCTION_ACCESS_TOKEN");
        std::env::remove_var("SQUARE_PRODUCTION_LOCATION_ID");
        std::env::remove_var("SQUARE_WEBHOOK_NOTIFICATION_URL");
        std::env::remove_var("SQUARE_WEBHOOK_SIGNATURE_KEY");
        std::env::remove_var("SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY");
        std::env::remove_var("SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY");
    }

    #[test]
    fn admin_email_matching_is_normalized() {
        let cfg = Config {
            port: 0,
            db_path: PathBuf::from(":memory:"),
            db_backend: ServerDbBackend::Sqlite,
            database_url: None,
            jwt_secret: "test-secret-at-least-32-chars-long".to_string(),
            public_url: "http://localhost".to_string(),
            stripe_secret_key: None,
            stripe_webhook_secret: None,
            upstream: UpstreamKeys::default(),
            upstream_spend_guard: None,
            smtp: None,
            admin_emails: parse_email_list(Some(" Owner@Bluey.SH , bad,ops@bluey.sh ".into())),
            trial_abuse: TrialAbuseConfig::default(),
            turnstile_site_key: None,
            turnstile_secret_key: None,
            require_turnstile: false,
            object_storage: None,
            log_storage: None,
        };

        assert!(cfg.is_admin_email("owner@bluey.sh"));
        assert!(cfg.is_admin_email(" OPS@BLUEY.SH "));
        assert!(!cfg.is_admin_email("user@bluey.sh"));
    }

    #[test]
    fn upstream_spend_guard_requires_a_positive_limit() {
        std::env::set_var("BLUEY_UPSTREAM_SPEND_LIMIT_CENTS", "1000");
        std::env::set_var("BLUEY_UPSTREAM_SPEND_WINDOW_HOURS", "12");
        let guard = upstream_spend_guard_from_env().unwrap();
        assert_eq!(guard.limit_cents, 1000);
        assert_eq!(guard.window_hours, 12);

        std::env::set_var("BLUEY_UPSTREAM_SPEND_LIMIT_CENTS", "0");
        assert!(upstream_spend_guard_from_env().is_none());

        std::env::remove_var("BLUEY_UPSTREAM_SPEND_LIMIT_CENTS");
        std::env::remove_var("BLUEY_UPSTREAM_SPEND_WINDOW_HOURS");
    }
}
