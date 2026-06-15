//! Cloud agent registry — data table for vendors that run an agent in the
//! cloud (Copilot Coding Agent, Cursor Cloud, Anthropic Managed Agents,
//! Codex Cloud).
//!
//! This sits alongside the existing local-agent [`crate::registry::REGISTRY`]
//! rather than extending its `AgentEntry` row, because cloud vendors have a
//! disjoint set of concerns (HTTP endpoints, billing model, async task
//! shape) and reusing the local row would force every vendor to carry
//! ignored fields (`binary_candidates`, `data_dir_globs`, …).
//!
//! ### Data-row philosophy (same as the local registry)
//!
//! Adding a cloud vendor = one [`CloudAgentEntry`] in [`CLOUD_REGISTRY`] +
//! one vendor adapter file. Per-vendor logic lives in the adapter
//! (`cloud/<vendor>.rs`); everything else is shared.
//!
//! Hard rules enforced by tests in this module:
//!
//! - Every row MUST declare [`billing_model`] — the disclosure UI reads it
//!   off the row, never special-cases by vendor name.
//! - Every row MUST declare a non-empty `consent_warning` — the enrollment
//!   UI MUST surface it before storing a credential.
//! - Every row MUST set `vendor_short` to a stable lowercase identifier used
//!   in audit logs and keychain service names.

use crate::registry::KindTag;

// Re-export each vendor adapter's ENTRY for the static `CLOUD_REGISTRY`
// table below. Adding a vendor = adding `&cloud::<vendor>::ENTRY` here.
use super::anthropic;
use super::antigravity_cloud;
use super::codex_cloud;
use super::copilot;
use super::cursor;
use super::gemini_cloud;

/// How a cloud vendor charges for use. The disclosure UI MUST surface this
/// before the user enrolls a credential. Data-driven: a new billing shape is
/// a new variant; the UI matches on the variant, never on a vendor name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingModel {
    /// Counts against a paid vendor subscription (GitHub Copilot Pro/Business,
    /// Cursor Pro/Business, etc.). UI disclosure: "this counts against your
    /// {vendor} subscription."
    Subscription,
    /// Pay-per-call API credits (Anthropic API, OpenAI API). UI disclosure:
    /// "you're billed per request by {vendor}."
    ApiCredits,
    /// "Bring your own token" — the user already pays the vendor under a
    /// plan we can't introspect. UI disclosure: "Bluey just relays your
    /// existing {vendor} credential."
    Byot,
}

impl BillingModel {
    /// Stable snake_case label for the UI / wire DTOs.
    pub fn as_str(self) -> &'static str {
        match self {
            BillingModel::Subscription => "subscription",
            BillingModel::ApiCredits => "api_credits",
            BillingModel::Byot => "byot",
        }
    }
}

/// Coarse purpose for an endpoint in a vendor's API surface. The adapter
/// names which paths exist; this enum lets cross-vendor code (e.g. an audit
/// label) refer to them by *intent* without naming a vendor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudEndpointKind {
    /// Create one task / session — the kick-off call.
    CreateTask,
    /// Fetch one task's status by id.
    GetTask,
    /// List recent tasks.
    ListTasks,
}

/// Generic state of a cloud task, normalized across vendors so the
/// daemon's polling/notification path stays vendor-agnostic.
///
/// Each adapter parses its vendor-specific status string (`queued`,
/// `in_progress`, `Running`, etc.) into one of these variants. The
/// `Other(String)` arm keeps an unrecognized status visible in the audit
/// log without breaking the parser — a new variant gets added in this enum
/// when the UI is ready to render it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudTaskState {
    /// Accepted but not yet running.
    Queued,
    /// Currently executing.
    Running,
    /// Waiting for a user action (input, approval, …).
    Waiting,
    /// Finished successfully — the result is ready.
    Succeeded,
    /// Finished with a failure (CI error, model error, permission denied).
    Failed,
    /// Cancelled by user or admin.
    Cancelled,
    /// Vendor returned a status the adapter doesn't recognize yet. The raw
    /// string is preserved for the audit log. NOT terminal — the poller
    /// should keep checking, since an unknown status might just be a new
    /// in-progress label.
    Other(String),
}

impl CloudTaskState {
    /// Whether this state means "no more progress will happen" — the poll
    /// loop should stop. `Other` is NOT terminal (we don't know what it is).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CloudTaskState::Succeeded | CloudTaskState::Failed | CloudTaskState::Cancelled
        )
    }

    /// Stable snake_case label for the UI / wire DTOs.
    pub fn as_str(&self) -> &str {
        match self {
            CloudTaskState::Queued => "queued",
            CloudTaskState::Running => "running",
            CloudTaskState::Waiting => "waiting",
            CloudTaskState::Succeeded => "succeeded",
            CloudTaskState::Failed => "failed",
            CloudTaskState::Cancelled => "cancelled",
            CloudTaskState::Other(s) => s.as_str(),
        }
    }
}

/// One row in the cloud-agent registry.
///
/// Pure data. The adapter (`cloud/<vendor>.rs`) carries the irreducible
/// per-vendor logic (request body shapes, status-string parsing). Everything
/// you can read off this row is shared machinery.
#[derive(Debug)]
pub struct CloudAgentEntry {
    /// Local-agent [`KindTag`] this row corresponds to. Cloud-only vendors
    /// add a new tag; cloud variants of an existing agent reuse the tag with
    /// a different `vendor_short` (e.g. `KindTag::CopilotCloud`).
    pub kind_tag: KindTag,
    /// Human-facing display name (e.g. "GitHub Copilot Coding Agent (Cloud)").
    pub display_name: &'static str,
    /// Vendor short identifier used in audit logs and keychain service names
    /// (e.g. `"copilot_cloud"`, `"cursor_cloud"`). Lowercase ASCII, no
    /// whitespace, no slashes.
    pub vendor_short: &'static str,
    /// Default API host (overridable at runtime for GHEC-with-residency,
    /// self-hosted GHES, on-prem variants). The transport reads this when
    /// the user hasn't supplied an override.
    pub base_url: &'static str,
    /// Billing disclosure shape. See [`BillingModel`].
    pub billing_model: BillingModel,
    /// Required warning text the enrollment UI MUST surface BEFORE storing a
    /// credential. Examples:
    /// - "Copilot Coding Agent runs on a GitHub Actions runner you pay for;
    ///   the PAT you provide will be stored in your OS keychain."
    /// - "Anthropic Managed Agents are billed per-request to your console."
    pub consent_warning: &'static str,
    /// `true` if this vendor is task-shaped: a single ask returns a long-
    /// running job, not a synchronous answer. The daemon's answer ladder
    /// reads this flag to decide between "wait for the stream" and "kick
    /// off + acknowledge + park for notification."
    pub task_shaped: bool,
    /// Vendor-documented maximum task duration in seconds (used to size
    /// poll timeouts and to tell the user when to expect a result).
    /// `0` for session-shaped vendors that don't have a hard task limit.
    pub max_task_duration_secs: u32,
}

/// The cloud-agent registry. **Adding a cloud vendor = adding a row here.**
///
/// Adapters re-export an `ENTRY: &'static CloudAgentEntry`; the row appears
/// in this slice in detection-priority order.
pub static CLOUD_REGISTRY: &[&CloudAgentEntry] = &[
    // GitHub Copilot Coding Agent (Cloud) — task-shaped, runs in Actions.
    copilot::ENTRY,
    // Cursor Cloud Agents (placeholder until the parallel agent lands the
    // real adapter — or the real one, depending on race order).
    cursor::ENTRY,
    // Anthropic Managed Agents (placeholder until the parallel agent lands
    // the real adapter).
    anthropic::ENTRY,
    // OpenAI Codex Cloud (task-shaped, BYOT OpenAI API key).
    codex_cloud::ENTRY,
    // Google Antigravity (Cloud) — session/turn-shaped, BYOT Gemini API key.
    // This is the Gemini API Managed Agents / Interactions endpoint; the
    // `agent` field (antigravity-preview-05-2026) is the sole differentiator
    // from a generic Gemini-cloud row. See `docs/vendors/antigravity_cloud.md`.
    antigravity_cloud::ENTRY,
    // Google Gemini Managed Agents (Cloud) — TURN-shaped (synchronous answer +
    // optional SSE), BYOT AI Studio Gemini API key (`x-goog-api-key`). The
    // generic Gemini cloud surface; shares the Interactions endpoint with the
    // Antigravity row above but is a distinct vendor identity. The only
    // turn-shaped row in this table. See `docs/vendors/gemini_cloud.md`.
    gemini_cloud::ENTRY,
];

/// Look up the cloud-registry row for a kind tag. Returns the first row
/// matching the tag; `None` if no cloud vendor is registered for that tag
/// (the agent might still have a local row in [`crate::registry::REGISTRY`]).
pub fn cloud_entry_for(kind: KindTag) -> Option<&'static CloudAgentEntry> {
    CLOUD_REGISTRY.iter().copied().find(|e| e.kind_tag == kind)
}

/// Iterate every cloud vendor row (for the UI's enrollment surface).
pub fn iter_cloud_entries() -> impl Iterator<Item = &'static CloudAgentEntry> {
    CLOUD_REGISTRY.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cloud_row_declares_a_billing_model() {
        // Hard requirement from the integration loop: every cloud row carries
        // a billing model so the disclosure UI never special-cases a vendor.
        for entry in CLOUD_REGISTRY {
            // Match every variant explicitly so a future BillingModel
            // variant fails this test on purpose (someone has to look).
            match entry.billing_model {
                BillingModel::Subscription | BillingModel::ApiCredits | BillingModel::Byot => {}
            }
        }
    }

    #[test]
    fn every_cloud_row_has_consent_warning() {
        for entry in CLOUD_REGISTRY {
            assert!(
                !entry.consent_warning.trim().is_empty(),
                "{} must declare a consent_warning",
                entry.display_name
            );
        }
    }

    #[test]
    fn every_cloud_row_has_lowercase_vendor_short() {
        for entry in CLOUD_REGISTRY {
            let v = entry.vendor_short;
            assert!(
                !v.is_empty(),
                "{} has empty vendor_short",
                entry.display_name
            );
            assert!(
                v.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{} vendor_short {:?} must be lowercase ASCII / digits / underscore",
                entry.display_name,
                v,
            );
        }
    }

    #[test]
    fn every_cloud_row_has_https_base_url() {
        for entry in CLOUD_REGISTRY {
            assert!(
                entry.base_url.starts_with("https://"),
                "{} base_url {:?} must be https://",
                entry.display_name,
                entry.base_url,
            );
        }
    }

    #[test]
    fn task_shaped_rows_declare_duration() {
        // A task-shaped vendor MUST declare a max_task_duration_secs > 0
        // so the daemon can size its poll loop.
        for entry in CLOUD_REGISTRY {
            if entry.task_shaped {
                assert!(
                    entry.max_task_duration_secs > 0,
                    "{} is task_shaped and must declare max_task_duration_secs > 0",
                    entry.display_name
                );
            }
        }
    }

    #[test]
    fn cloud_task_state_terminal_classification() {
        assert!(CloudTaskState::Succeeded.is_terminal());
        assert!(CloudTaskState::Failed.is_terminal());
        assert!(CloudTaskState::Cancelled.is_terminal());
        assert!(!CloudTaskState::Queued.is_terminal());
        assert!(!CloudTaskState::Running.is_terminal());
        assert!(!CloudTaskState::Waiting.is_terminal());
        // Other is non-terminal — unknown states might just be in-progress.
        assert!(!CloudTaskState::Other("weird".to_string()).is_terminal());
    }

    #[test]
    fn billing_model_labels_are_stable() {
        // The wire label is a public contract; pinning the strings here
        // prevents silent breakage.
        assert_eq!(BillingModel::Subscription.as_str(), "subscription");
        assert_eq!(BillingModel::ApiCredits.as_str(), "api_credits");
        assert_eq!(BillingModel::Byot.as_str(), "byot");
    }

    #[test]
    fn cloud_task_state_labels_are_stable() {
        assert_eq!(CloudTaskState::Queued.as_str(), "queued");
        assert_eq!(CloudTaskState::Running.as_str(), "running");
        assert_eq!(CloudTaskState::Waiting.as_str(), "waiting");
        assert_eq!(CloudTaskState::Succeeded.as_str(), "succeeded");
        assert_eq!(CloudTaskState::Failed.as_str(), "failed");
        assert_eq!(CloudTaskState::Cancelled.as_str(), "cancelled");
        assert_eq!(CloudTaskState::Other("xyz".to_string()).as_str(), "xyz");
    }
}
