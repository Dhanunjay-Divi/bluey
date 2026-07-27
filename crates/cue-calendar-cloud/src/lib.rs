//! Cloud OAuth calendar sources for Bluey.
//!
//! A native public-client OAuth flow (RFC 8252): PKCE (S256) + a transient
//! loopback redirect, `client_id` only (NO client secret), tokens stored in the
//! OS keychain per provider. A connected cloud calendar becomes a new
//! `cue_core::calendar::CalendarSource` so the daemon's warmup trigger loop,
//! dedupe, and warm-drive are unchanged.
//!
//! This crate depends on `cue-core` (for the shared calendar types) and NEVER
//! on `cue-daemon`, so it can be pulled in behind cue-daemon's `cloud-calendar`
//! feature without a dependency cycle.
//!
//! Module map:
//! - [`pkce`] — RFC 7636 PKCE (S256) verifier/challenge + random state (pure).
//! - [`provider`] — per-provider endpoints/scopes/client-id (pure data).
//! - [`authorize`] — build the authorize URL (pure).
//! - [`loopback`] — the transient single-shot redirect listener (async I/O).
//! - [`tokens`] — keyring-backed per-provider token store.
//! - [`oauth`] — the flow driver: exchange/refresh + interactive connect.

pub mod authorize;
pub mod loopback;
pub mod oauth;
pub mod pkce;
pub mod provider;
pub mod tokens;

pub mod google;
pub mod microsoft;

// --- Public API (code against these exact paths) -------------------------

pub use oauth::{
    connect_interactive, exchange_code, refresh, valid_access_token, valid_access_token_serialized,
    TokenResponse,
};
pub use pkce::{random_state, Pkce};
pub use provider::{Provider, ProviderConfig};
pub use tokens::{
    is_expired, CachedCalStore, CalTokenStore, CalTokens, KeyringCalStore, MemoryCalStore,
    DEFAULT_EXPIRY_SKEW_SECS,
};

// Re-export the shared calendar types so B/C (and any consumer) can name them
// without also depending on cue-core directly.
pub use cue_core::calendar::{CalendarSource, Participant, UpcomingEvent};
