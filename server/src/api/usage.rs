//! Usage event ingestion. Daemon emits a usage event after every cue.
//!
//! Incoming fields are client-supplied and never enter authoritative spend
//! accounting. Only a small analytics allowlist is accepted; its kind is
//! namespaced and provider/model/cost fields are erased before persistence.

use axum::{extract::State, http::StatusCode, Extension, Json};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::usage::{self, UsageEvent};

pub async fn ingest(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(event): Json<UsageEvent>,
) -> StatusCode {
    if account.billing_restricted {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            reason = account
                .billing_restriction_reason
                .as_deref()
                .unwrap_or("billing_restricted"),
            "billing-restricted account blocked from usage ingestion"
        );
        return StatusCode::FORBIDDEN;
    }
    let Some(event) = sanitize_client_analytics_event(event) else {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            "rejected non-analytics client usage event"
        );
        return StatusCode::UNPROCESSABLE_ENTITY;
    };
    match usage::record_client_analytics(&state.pool, &account.id, &event) {
        Ok(true) => StatusCode::ACCEPTED,
        // Codex Stage 7 S7.1: replay -> 200 OK. The caller knows the
        // request was already accepted; this is the standard idempotent
        // semantic. Returning 202 again would be confusing because the
        // event is no longer "newly accepted".
        Ok(false) => StatusCode::OK,
        Err(e) => {
            tracing::warn!(error = %e, account_id_hash = %cue_core::account_id_hash_prefix(&account.id), "usage event record failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn sanitize_client_analytics_event(mut event: UsageEvent) -> Option<UsageEvent> {
    let suffix = match event.kind.as_str() {
        "local_llm" | "local_ollama" => "local_llm",
        "client_timing" => "timing",
        _ => return None,
    };
    if event.request_id.trim().is_empty() || event.request_id.len() > 256 {
        return None;
    }
    event.kind = format!("client_analytics:{suffix}");
    event.task_type = None;
    event.lane = None;
    event.provider = None;
    event.model = None;
    event.input_tokens = event.input_tokens.clamp(0, 100_000_000);
    event.output_tokens = event.output_tokens.clamp(0, 100_000_000);
    event.latency_ms = event.latency_ms.clamp(0, 86_400_000);
    event.cost_cents_to_bluey = 0;
    event.cost_cents_to_customer = 0;
    event.was_speculative = false;
    event.was_fallback = false;
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_event(kind: &str) -> UsageEvent {
        UsageEvent {
            request_id: "request-1".to_string(),
            kind: kind.to_string(),
            task_type: Some("local".to_string()),
            lane: Some("local".to_string()),
            provider: Some("forged-provider".to_string()),
            model: Some("forged-model".to_string()),
            input_tokens: -1,
            output_tokens: i64::MAX,
            latency_ms: i64::MAX,
            cost_cents_to_bluey: i64::MIN,
            cost_cents_to_customer: i64::MAX,
            was_speculative: true,
            was_fallback: true,
        }
    }

    #[test]
    fn authoritative_usage_kinds_cannot_be_preseeded_by_clients() {
        for kind in [
            "llm",
            "embed",
            "stt",
            "web_search",
            "answer_plan_classifier",
        ] {
            assert!(sanitize_client_analytics_event(client_event(kind)).is_none());
        }
    }

    #[test]
    fn client_analytics_are_namespaced_and_stripped_of_spend_authority() {
        let event = sanitize_client_analytics_event(client_event("local_llm")).unwrap();
        assert_eq!(event.kind, "client_analytics:local_llm");
        assert_eq!(event.cost_cents_to_bluey, 0);
        assert_eq!(event.cost_cents_to_customer, 0);
        assert_eq!(event.input_tokens, 0);
        assert_eq!(event.output_tokens, 100_000_000);
        assert_eq!(event.latency_ms, 86_400_000);
        assert!(event.provider.is_none());
        assert!(event.model.is_none());
        assert!(event.task_type.is_none());
        assert!(event.lane.is_none());
        assert!(!event.was_speculative);
        assert!(!event.was_fallback);
    }
}
