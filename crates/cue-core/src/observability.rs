//! Shared observability primitives for Bluey clients, daemon, and server.
//!
//! This module intentionally stays small: stable header names, stable
//! non-PII account hashing, standard log fields, and a thin `observe!`
//! macro that expands to `tracing::event!`.

use std::fmt::Write as _;

use uuid::Uuid;

/// Standard trace header propagated across UI/daemon/cloud/server hops.
pub const BLUEY_TRACE_ID_HEADER: &str = "x-bluey-trace-id";
/// Standard per-hop request header. Server echoes this header on response.
pub const BLUEY_REQUEST_ID_HEADER: &str = "x-bluey-request-id";
/// Per-user-interaction correlation header propagated across answer hops.
///
/// Unlike trace and request ids, interaction ids are always UUIDs minted at
/// the UI boundary and must never be derived from user content.
pub const BLUEY_INTERACTION_ID_HEADER: &str = "x-bluey-interaction-id";
/// Environment fallback used by CLI-launched flows before Phase 5's explicit
/// Tauri/IPC trace propagation lands.
pub const BLUEY_TRACE_ID_ENV: &str = "BLUEY_TRACE_ID";

/// Stable, redaction-safe fields that can be attached to Bluey log events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserveFields {
    pub component: String,
    pub version: String,
    pub platform: String,
    pub trace_id: Option<String>,
    pub request_id: Option<String>,
    pub interaction_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id_hash: Option<String>,
    pub status: Option<String>,
    pub latency_ms: Option<u64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub cost_cents_to_customer: Option<i64>,
    pub cost_cents_to_bluey: Option<i64>,
}

impl ObserveFields {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: platform(),
            trace_id: None,
            request_id: None,
            interaction_id: None,
            session_id: None,
            account_id_hash: None,
            status: None,
            latency_ms: None,
            provider: None,
            model: None,
            cost_cents_to_customer: None,
            cost_cents_to_bluey: None,
        }
    }

    pub fn trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = sanitize_interaction_id(&trace_id.into());
        self
    }

    pub fn request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn interaction_id(mut self, interaction_id: impl Into<String>) -> Self {
        self.interaction_id = sanitize_interaction_id(&interaction_id.into());
        self
    }

    pub fn session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn account_id(mut self, account_id: impl AsRef<str>) -> Self {
        self.account_id_hash = Some(account_id_hash_prefix(account_id.as_ref()));
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    pub fn latency_ms(mut self, latency_ms: u64) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }

    pub fn provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn customer_cost_cents(mut self, cost: i64) -> Self {
        self.cost_cents_to_customer = Some(cost);
        self
    }

    pub fn bluey_cost_cents(mut self, cost: i64) -> Self {
        self.cost_cents_to_bluey = Some(cost);
        self
    }

    pub fn trace_id_value(&self) -> &str {
        self.trace_id.as_deref().unwrap_or("")
    }

    pub fn request_id_value(&self) -> &str {
        self.request_id.as_deref().unwrap_or("")
    }

    pub fn interaction_id_value(&self) -> &str {
        self.interaction_id.as_deref().unwrap_or("")
    }

    pub fn session_id_value(&self) -> &str {
        self.session_id.as_deref().unwrap_or("")
    }

    pub fn account_id_hash_value(&self) -> &str {
        self.account_id_hash.as_deref().unwrap_or("")
    }

    pub fn status_value(&self) -> &str {
        self.status.as_deref().unwrap_or("")
    }

    pub fn provider_value(&self) -> &str {
        self.provider.as_deref().unwrap_or("")
    }

    pub fn model_value(&self) -> &str {
        self.model.as_deref().unwrap_or("")
    }
}

pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub fn new_trace_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn new_request_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn new_interaction_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn trace_id_from_env() -> Option<String> {
    std::env::var(BLUEY_TRACE_ID_ENV)
        .ok()
        .and_then(|value| sanitize_interaction_id(&value))
}

pub fn sanitize_observability_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return None;
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        Some(value.to_string())
    } else {
        None
    }
}

/// Validate a privacy-safe interaction id and return its canonical UUID form.
///
/// Interaction ids intentionally use a stricter contract than request and
/// trace ids so free-form values cannot become a cross-system logging field.
pub fn sanitize_interaction_id(value: &str) -> Option<String> {
    Uuid::parse_str(value.trim())
        .ok()
        .map(|interaction_id| interaction_id.to_string())
}

/// Stable 8-character support reference for request/session ids.
///
/// This is intentionally not a secret. It is a human-friendly join key for
/// screenshots, support bundles, redacted audit rows, and logs.
pub fn short_observability_ref(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "NONE".to_string();
    };
    let trimmed = value.trim();
    let compact = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if compact.is_empty() {
        return "NONE".to_string();
    }

    let short = if looks_like_uuid_reference(trimmed) || compact.len() <= 8 {
        compact.chars().take(8).collect::<String>()
    } else {
        let mut tail = compact.chars().rev().take(8).collect::<Vec<_>>();
        tail.reverse();
        tail.into_iter().collect::<String>()
    };
    if short.is_empty() {
        "NONE".to_string()
    } else {
        short.to_ascii_uppercase()
    }
}

fn looks_like_uuid_reference(value: &str) -> bool {
    let value = value.trim();
    let prefix = value.chars().take(36).collect::<Vec<_>>();
    if prefix.len() < 36 {
        return false;
    }
    prefix.iter().enumerate().all(|(idx, ch)| match idx {
        8 | 13 | 18 | 23 => *ch == '-',
        _ => ch.is_ascii_hexdigit(),
    })
}

/// SHA-256 of account id, first 12 hex chars. This is the support join key
/// used by doctor/logs without placing raw account ids in local logs.
pub fn account_id_hash_prefix(account_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(account_id.as_bytes());
    let hash = hasher.finalize();
    let mut out = String::with_capacity(12);
    for byte in &hash[..6] {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_hash_is_stable_short_and_hex() {
        let hash = account_id_hash_prefix("acct_12345");
        assert_eq!(hash.len(), 12);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash, account_id_hash_prefix("acct_12345"));
        assert_ne!(hash, account_id_hash_prefix("acct_other"));
    }

    #[test]
    fn sanitize_observability_id_rejects_bad_values() {
        assert_eq!(
            sanitize_observability_id(" trace-123 "),
            Some("trace-123".to_string())
        );
        assert_eq!(sanitize_observability_id("bad\nid"), None);
        assert_eq!(sanitize_observability_id(""), None);
        assert_eq!(sanitize_observability_id(&"x".repeat(129)), None);
    }

    #[test]
    fn interaction_ids_are_uuid_only_and_canonical() {
        let generated = new_interaction_id();
        assert!(Uuid::parse_str(&generated).is_ok());
        assert_eq!(
            sanitize_interaction_id(" 550E8400-E29B-41D4-A716-446655440000 "),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(sanitize_interaction_id("interaction-123"), None);
        assert_eq!(sanitize_interaction_id("person@example.com"), None);
        assert_eq!(sanitize_interaction_id("550e8400\ne29b"), None);
    }

    #[test]
    fn short_observability_ref_matches_session_screenshot_codes() {
        assert_eq!(
            short_observability_ref(Some("25594f6d-4cc7-4315-b99b-017b567851ae")),
            "25594F6D"
        );
        assert_eq!(
            short_observability_ref(Some("74c0a385-e56a-4afd-bb90-5abb4941cebb")),
            "74C0A385"
        );
        assert_eq!(short_observability_ref(None), "NONE");
        assert_eq!(short_observability_ref(Some(" --- ")), "NONE");
    }

    #[test]
    fn short_observability_ref_uses_entropy_suffix_for_prefixed_request_ids() {
        assert_eq!(
            short_observability_ref(Some("live-eval-20260706-quick-concept-a1b2c3d4")),
            "A1B2C3D4"
        );
        assert_eq!(
            short_observability_ref(Some("live-eval-20260706-system-design-e5f60718")),
            "E5F60718"
        );
    }

    #[test]
    fn observe_fields_builder_sets_standard_values() {
        const TRACE_ID: &str = "550e8400-e29b-41d4-a716-446655440001";
        let fields = ObserveFields::new("cue-daemon")
            .trace_id(TRACE_ID)
            .request_id("request")
            .interaction_id("550e8400-e29b-41d4-a716-446655440000")
            .account_id("acct_123")
            .status("ok")
            .latency_ms(42)
            .provider("bluey-managed")
            .model("auto");
        assert_eq!(fields.component, "cue-daemon");
        assert_eq!(fields.trace_id_value(), TRACE_ID);
        assert_eq!(fields.request_id_value(), "request");
        assert_eq!(
            fields.interaction_id_value(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(fields.status_value(), "ok");
        assert_eq!(fields.latency_ms, Some(42));
        assert_eq!(fields.provider_value(), "bluey-managed");
        assert_eq!(fields.model_value(), "auto");
        assert_eq!(fields.account_id_hash_value().len(), 12);
    }

    #[test]
    fn observe_macro_compiles_with_standard_fields() {
        crate::observe!(
            tracing::Level::INFO,
            ObserveFields::new("cue-core")
                .trace_id("550e8400-e29b-41d4-a716-446655440001")
                .request_id("request")
                .interaction_id("550e8400-e29b-41d4-a716-446655440000")
                .status("ok"),
            "observability smoke"
        );
    }
}
