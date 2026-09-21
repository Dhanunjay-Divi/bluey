//! cue-cloud-client: HTTPS client the Bluey daemon uses to talk to
//! `bluey-server`.
//!
//! Concerns:
//!   - Auth flows: device login (preferred for the daemon), token
//!     storage in Bluey's private account profile by default, automatic
//!     refresh on 401.
//!   - Managed Auto Router dispatch: `/router/complete`, `/router/embed`,
//!     `/router/transcribe` — when wired into `BlueyManagedProvider` (R14.11).
//!   - Account state: `/account/me` (balance, trial, auto-topup config).
//!   - Usage events: `/usage/event` per cue.
//!
//! Design notes:
//!   - All endpoints return strongly-typed Rust structs.
//!   - 401 triggers automatic token refresh + one retry; subsequent 401
//!     surfaces as `Error::Unauthorized` so the caller can prompt
//!     first-run sign-in from `bluey on`.
//!   - 402 (Payment Required) surfaces as `Error::InsufficientBalance`
//!     so the daemon can show the "Add $30" banner.
//!   - 429 (Rate Limited) surfaces with the Retry-After hint.
//!
//! See `ARCHITECTURE.md` Section 6 + `docs/HOW-IT-WORKS.md` for the
//! full v0.2 customer flow.

#![forbid(unsafe_code)]
// #![warn(missing_docs)]  // re-enable once all fields are documented

pub mod auth;
pub mod client;
pub mod error;
pub mod tokens;
pub mod types;

pub use auth::{DeviceFlow, DeviceFlowState};
pub use client::{CloudClient, CredentialFreeHttpTransport};
pub use error::Error;
pub use tokens::{
    save_account_profile_and_tokens, save_account_profile_and_tokens_if_generation,
    save_account_profile_without_tokens, AccountFileStore, CredentialAuthority, CredentialSnapshot,
    SecureAccountStore, TokenStore, Tokens,
};
pub use types::*;
