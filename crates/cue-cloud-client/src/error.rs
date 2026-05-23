//! Error types for cue-cloud-client.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    /// Network / transport error.
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON ser/de error.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// 401 — caller should prompt first-run sign-in from `bluey on`.
    #[error("unauthorized: token invalid or expired")]
    Unauthorized,

    /// 402 — wallet balance insufficient. The included field is the
    /// server's reason payload so the daemon can surface a clear UI banner.
    #[error("insufficient balance (need {needed_cents} cents, have {balance_cents})")]
    InsufficientBalance {
        balance_cents: i64,
        needed_cents: i64,
        reload_url: String,
    },

    /// 429 — rate limited. Retry-After in seconds.
    #[error("rate limited (retry after {retry_after_secs}s)")]
    RateLimited { retry_after_secs: u64 },

    /// Trial ended (server returns 402 with reason="trial_ended").
    #[error("trial ended; first $30 reload required")]
    TrialEnded,

    /// Other server errors (5xx, malformed responses, etc.).
    /// Codex Stage 9d (S8.4 round-2 nit): body intentionally NOT
    /// surfaced to consumers — production error bodies can leak
    /// provider-internal details. Raw body is logged at warn level
    /// inside `client::parse_or_err` / `auth::*` where the error is
    /// constructed.
    #[error("server error {status}")]
    Server { status: u16 },

    /// Keyring / token storage error.
    #[error("token store: {0}")]
    TokenStore(String),

    /// Generic anyhow-like fallback.
    #[error("{0}")]
    Other(String),
}

impl From<keyring::Error> for Error {
    fn from(e: keyring::Error) -> Self {
        Error::TokenStore(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
