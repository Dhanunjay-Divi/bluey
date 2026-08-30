//! bluey-server: production server for the v0.2 Bluey paid product.
//!
//! Provides:
//!   - Auth (signup, login, refresh, device flow for daemon login)
//!   - Billing (Stripe checkout, webhook, balance + per-batch credits)
//!   - Managed Auto Router endpoint (proxies LLM / embedding / STT calls
//!     through Bluey-owned API keys; meters per request)
//!   - Account dashboard + usage endpoints
//!   - Admin endpoints (Bluey-team only)
//!
//! Mirrors Pinky's operational shape (single binary + SQLite + Caddy +
//! LetsEncrypt + droplet) but written in Rust per the 2026-05-19
//! decision. Lives in this repo at `server/` for now; will split to a
//! separate `bluey-server` repo once it stabilises.
//!
//! See ../ARCHITECTURE.md, ../docs/HOW-IT-WORKS.md, ../docs/PRICING-MODEL.md.

#![forbid(unsafe_code)]

#[cfg(all(feature = "integration-test-support", not(debug_assertions)))]
compile_error!("integration-test-support must never be enabled in release builds");

#[cfg(all(
    feature = "business-messaging-simulator-test-support",
    not(debug_assertions)
))]
compile_error!("business-messaging-simulator-test-support must never be enabled in release builds");

pub mod api;
pub mod auth;
pub mod billing;
pub mod config;
pub mod db;
pub(crate) mod jobs_ats_target;
#[cfg(feature = "business-messaging-simulator-test-support")]
pub mod jobs_business_messaging_simulator;
pub mod jobs_communication_dispatch;
pub mod jobs_global_archive;
pub mod jobs_mailbox_sync;
pub mod jobs_managed_cloud_runtime;
pub(crate) mod jobs_provider_auth;
pub mod jobs_resume_template;
pub mod jobs_taxonomy;
pub mod jobs_workflow_cleanup;
pub mod jobs_workflow_dispatch;
pub mod mail;
pub mod object_storage;
pub mod pricing;
pub mod provider_health;
pub mod rate_limit;
pub mod routing;

pub use config::Config;
