//! Cloud agents — shared primitives for vendors whose agent runs in the
//! vendor's cloud (Cursor Cloud, Copilot Cloud, Anthropic, Codex Cloud).
//!
//! ### Coordination convention
//!
//! Four agents are running this loop in parallel — Cursor, Copilot, Anthropic,
//! and Codex Cloud — and they all need the same three primitives: an HTTP
//! transport that hides auth header construction, an OS-keychain store for
//! credentials, and a structured audit log. To avoid clobbering each other:
//!
//! - The shared primitives ([`transport`], [`keychain`], [`audit`]) are
//!   written **extensibly**: any new auth scheme is a new variant on
//!   [`transport::CloudAuth`], any new vendor is a new keychain service name,
//!   etc. Do NOT delete or repurpose what's already here — extend it.
//! - The per-vendor adapter (e.g. [`anthropic`]) is the only place that
//!   names a vendor inline. Everything generic stays vendor-agnostic.
//! - New cloud agents add a vendor adapter file *next to* `anthropic.rs`,
//!   register themselves in this `mod.rs`, and append their registry row.
//!
//! ### Hard rules
//!
//! - Tokens MUST go through [`keychain`]. Never plaintext disk, never an env
//!   var the daemon writes back out.
//! - Every cloud HTTP call MUST emit one [`audit::emit`] line. The audit
//!   helper guarantees the token is never reflected.
//! - Every registry row that points at a cloud vendor MUST set
//!   `billing_model` — the disclosure UI reads it off the row, never
//!   special-cases by name.
//!
//! ### Module map
//!
//! - [`transport`] — data-driven HTTPS transport ([`transport::CloudTransport`]
//!   enum that hangs off `AgentEntry`, [`transport::CloudAuth`] for the auth
//!   header shape, [`transport::CloudEndpoints`] for the URL paths).
//! - [`keychain`] — OS keyring wrapper, vendor-agnostic
//!   ([`keychain::VendorCredentialStore`] trait, [`keychain::KeychainCredentialStore`]
//!   production impl, [`keychain::MemoryCredentialStore`] for tests).
//! - [`audit`] — vendor-agnostic structured log line for every cloud call
//!   ([`audit::AuditEvent`], [`audit::emit`]).
//! - [`anthropic`] — Anthropic Managed Agents adapter (SSE event parsing,
//!   request-body builders, `sk-ant-oat01-*` rejection).
//! - [`copilot`] — GitHub Copilot Coding Agent adapter (task-shaped:
//!   `POST /agents/repos/{owner}/{repo}/tasks` + poll for the resulting PR).
//! - [`cursor`] — Cursor Cloud Agents adapter (task-shaped:
//!   `POST /v1/agents` → SSE stream of `/runs/{runId}/stream` events).
//! - [`codex_cloud`] — OpenAI Codex Cloud adapter (task-shaped:
//!   `POST /v1/codex/cloud/tasks` + poll `/v1/codex/cloud/tasks/{id}`).

pub mod anthropic;
pub mod antigravity_cloud;
pub mod audit;
pub mod codex_cloud;
pub mod copilot;
pub mod cursor;
pub mod drive;
pub mod gemini_cloud;
pub mod keychain;
pub mod registry;
pub mod transport;

pub use audit::{emit as audit_emit, AuditEvent};
pub use drive::drive_cloud;

/// Whether `kind` resolves to a cloud-vendor row in [`CLOUD_REGISTRY`].
///
/// Pure data lookup — the daemon's answer ladder uses this to pick between
/// the local-CLI drive and [`drive_cloud`] without naming a vendor.
pub fn is_cloud_kind(kind: &crate::AgentKind) -> bool {
    crate::registry::KindTag::from_agent_kind(kind)
        .and_then(cloud_entry_for)
        .is_some()
}
pub use keychain::{KeychainCredentialStore, MemoryCredentialStore, VendorCredentialStore};
pub use registry::{
    cloud_entry_for, BillingModel, CloudAgentEntry, CloudEndpointKind, CloudTaskState,
    CLOUD_REGISTRY,
};
pub use transport::{
    BasicAuthApiKey, BearerAuth, CloudAuth, CloudEndpoints, CloudHeaders, CloudHttpsTransport,
    CloudTransport, HttpRequest, HttpResponse,
};
