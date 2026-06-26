//! Usage event ingestion. Daemon emits a usage event after every cue.
//!
//! **Codex Stage 10 / S7.3 (round-2 nit):** the cost_cents_to_bluey,
//! cost_cents_to_customer, provider, and model fields on incoming
//! `/usage/event` requests are CLIENT-SUPPLIED and therefore UNTRUSTED.
//! /router/complete writes its own authoritative usage_event row with
//! the same request_id (Stage 4 idempotency wired both endpoints to
//! the same id). The /account/usage SQL aggregation reads the union;
//! UNIQUE(account_id, request_id, kind) means router-side rows take
//! precedence on tie because they arrive first.
//!
//! In practice, daemon-side /usage/event is now only useful for
//! *analytics-only* counters that the server can't observe (e.g.
//! local-Ollama spend with cost=$0, time-on-task histograms). For
//! billing/tier projection, treat the server-recorded row as
//! authoritative.

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
    match usage::record(&state.pool, &account.id, &event) {
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
