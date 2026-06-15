//! Structured audit log for cloud vendor calls.
//!
//! One [`tracing::info!`]-level line per HTTP call to a vendor's API. The
//! emitter is **vendor-agnostic** (it never names a vendor inline) and is
//! engineered so the credential cannot be reflected accidentally:
//!
//! - The function takes an [`AuditEvent`] struct whose fields are an explicit
//!   allow-list (vendor name, endpoint path, HTTP method, status code,
//!   latency, request-id presence). A token field does not exist.
//! - The struct holds **no** request/response body — bodies can carry both
//!   the user's prompt and (when echoed back) the credential.
//! - There is one Display + tracing surface; both write the same shape.
//!
//! ### What this records — and what it deliberately omits
//!
//! Recorded: `vendor`, `endpoint`, `method`, `status`, `latency_ms`, and a
//! coarse boolean `request_id_present` for the vendor's correlation id if it
//! came back in a response header. That's enough to investigate "did Bluey
//! talk to Cursor at 14:32, did it fail, was it slow?" without ever holding
//! payload or auth material.
//!
//! Omitted (BY DESIGN): the API key, the prompt text, the repo URL body, the
//! response body. A future structured field that holds any of those would
//! defeat the contract — extend the allow-list deliberately.

use std::time::Duration;

/// One audit event for a cloud vendor HTTP call. Construct via
/// [`AuditEvent::new`], hand to [`emit`].
#[derive(Debug, Clone)]
pub struct AuditEvent {
    /// Vendor short name, e.g. `"cursor"`. Lowercase ASCII, no whitespace.
    pub vendor: &'static str,
    /// Endpoint path, e.g. `"/v1/agents"`. NEVER the full URL (host is also
    /// boring constant data per-vendor and would just add noise).
    pub endpoint: &'static str,
    /// HTTP method, uppercase (`"GET"`, `"POST"`, …).
    pub method: &'static str,
    /// HTTP status code received, or `None` if the request never reached the
    /// server (connect/DNS failure).
    pub status: Option<u16>,
    /// Wall-clock time from request build to response headers received.
    pub latency: Duration,
    /// Whether the response carried a vendor-side correlation id header
    /// (e.g. `x-request-id`). The id ITSELF is not retained — only whether
    /// one was present, which is enough to know "we could correlate with
    /// the vendor if we had to."
    pub request_id_present: bool,
}

impl AuditEvent {
    /// Construct an event with the given fields.
    pub fn new(
        vendor: &'static str,
        endpoint: &'static str,
        method: &'static str,
        status: Option<u16>,
        latency: Duration,
        request_id_present: bool,
    ) -> Self {
        Self {
            vendor,
            endpoint,
            method,
            status,
            latency,
            request_id_present,
        }
    }
}

/// Emit one audit line for a cloud call. The output goes through
/// [`tracing::info!`] so the daemon's existing tracing-subscriber stack
/// picks it up alongside every other structured log.
///
/// The format is **structured fields**, not a string — so a future log
/// aggregator can pivot on `vendor` / `status` without parsing free text.
pub fn emit(event: &AuditEvent) {
    tracing::info!(
        target: "bluey.cloud_audit",
        vendor = event.vendor,
        endpoint = event.endpoint,
        method = event.method,
        status = event.status,
        latency_ms = event.latency.as_millis() as u64,
        request_id_present = event.request_id_present,
        "cloud agent call",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity: the struct's debug surface never contains a token-like field.
    /// We assert against the struct's own [`Debug`] (which is what tracing
    /// would render if we ever logged the struct whole).
    #[test]
    fn test_audit_event_debug_carries_no_secret_shape() {
        let ev = AuditEvent::new(
            "cursor",
            "/v1/agents",
            "POST",
            Some(201),
            Duration::from_millis(120),
            true,
        );
        let dbg = format!("{ev:?}");
        // Spot the fields that exist.
        assert!(dbg.contains("vendor"));
        assert!(dbg.contains("cursor"));
        assert!(dbg.contains("/v1/agents"));
        // And the fields that MUST NOT exist (defense-in-depth against a
        // future Debug-impl drift).
        for forbidden in ["token", "api_key", "authorization", "Bearer", "Basic"] {
            assert!(
                !dbg.to_lowercase().contains(&forbidden.to_lowercase()),
                "AuditEvent Debug must not reflect any secret-shaped field; found {forbidden:?} in {dbg}"
            );
        }
    }

    #[test]
    fn test_audit_event_construction_round_trips_fields() {
        let ev = AuditEvent::new(
            "cursor",
            "/v1/me",
            "GET",
            None,
            Duration::from_millis(7),
            false,
        );
        assert_eq!(ev.vendor, "cursor");
        assert_eq!(ev.endpoint, "/v1/me");
        assert_eq!(ev.method, "GET");
        assert_eq!(ev.status, None);
        assert_eq!(ev.latency.as_millis(), 7);
        assert!(!ev.request_id_present);
    }
}
