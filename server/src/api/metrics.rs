//! Codex Stage 15: /admin/metrics — Prometheus exposition format.
//!
//! Plain-text format, sourced from SQL counts so we do not need an
//! in-memory metrics library. Lightweight; sub-50ms even with the
//! whole table scan.
//!
//! Metrics:
//!
//!   bluey_accounts_total              - total accounts
//!   bluey_balance_cents_sum           - sum of all account balances
//!   bluey_trial_active_accounts       - accounts with trial_seconds > 0
//!   bluey_request_idempotency_total   - all request_idempotency rows
//!   bluey_request_idempotency_complete - rows in 'complete' state
//!   bluey_request_idempotency_in_progress - rows still in_progress
//!   bluey_mark_complete_failures_estimated - rows in_progress > 5 min
//!     (a proxy for Codex S4 nit: mark_complete crashed and left a
//!      reservation orphaned)
//!   bluey_credit_batches_total        - all credit_batches rows
//!   bluey_usage_events_24h            - usage_events in last 24h
//!   bluey_stripe_webhook_processed    - rows with processed_at NOT NULL
//!   bluey_provider_key_cooldowns_total - upstream key cooldown events
//!   bluey_provider_key_all_cooling_total - all keys cooling for a route
//!   bluey_provider_health_redis_errors_total - Redis health-ledger failures

use axum::{extract::State, http::StatusCode, response::IntoResponse};

use super::AppState;
use crate::db;

pub async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = match db::metrics::snapshot(&state.pool) {
        Ok(metrics) => metrics,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("# metrics error: {e}"),
            );
        }
    };
    let provider_health = state.provider_health.snapshot();
    let provider_key_cooldowns = provider_health.cooldowns_total;
    let provider_key_all_cooling = provider_health.all_keys_cooling_total;
    let provider_health_redis_errors = provider_health.redis_errors_total;
    let accounts = metrics.accounts;
    let balance_sum = metrics.balance_sum;
    let trial_active = metrics.trial_active;
    let req_total = metrics.request_idempotency_total;
    let req_complete = metrics.request_idempotency_complete;
    let req_in_progress = metrics.request_idempotency_in_progress;
    let mark_complete_failed = metrics.mark_complete_failed;
    let credit_batches = metrics.credit_batches;
    let usage_24h = metrics.usage_24h;
    let webhook_processed = metrics.webhook_processed;

    let body = format!(
        "# HELP bluey_accounts_total Total customer accounts.
         # TYPE bluey_accounts_total gauge
         bluey_accounts_total {accounts}
         # HELP bluey_balance_cents_sum Sum of all account balance_cents.
         # TYPE bluey_balance_cents_sum gauge
         bluey_balance_cents_sum {balance_sum}
         # HELP bluey_trial_active_accounts Accounts with trial_seconds_remaining > 0.
         # TYPE bluey_trial_active_accounts gauge
         bluey_trial_active_accounts {trial_active}
         # HELP bluey_request_idempotency_total Total request_idempotency rows.
         # TYPE bluey_request_idempotency_total counter
         bluey_request_idempotency_total {req_total}
         # HELP bluey_request_idempotency_complete Rows in complete state.
         # TYPE bluey_request_idempotency_complete counter
         bluey_request_idempotency_complete {req_complete}
         # HELP bluey_request_idempotency_in_progress Rows still in_progress.
         # TYPE bluey_request_idempotency_in_progress gauge
         bluey_request_idempotency_in_progress {req_in_progress}
         # HELP bluey_mark_complete_failures_estimated In-progress rows older than 5 min (proxy for mark_complete crash).
         # TYPE bluey_mark_complete_failures_estimated counter
         bluey_mark_complete_failures_estimated {mark_complete_failed}
         # HELP bluey_credit_batches_total Total credit_batches rows.
         # TYPE bluey_credit_batches_total counter
         bluey_credit_batches_total {credit_batches}
         # HELP bluey_usage_events_24h Usage events recorded in last 24h.
         # TYPE bluey_usage_events_24h counter
         bluey_usage_events_24h {usage_24h}
         # HELP bluey_stripe_webhook_processed Stripe webhook events successfully processed.
         # TYPE bluey_stripe_webhook_processed counter
         bluey_stripe_webhook_processed {webhook_processed}
         # HELP bluey_provider_key_cooldowns_total Provider/model/key cooldowns recorded after upstream capacity responses.
         # TYPE bluey_provider_key_cooldowns_total counter
         bluey_provider_key_cooldowns_total {provider_key_cooldowns}
         # HELP bluey_provider_key_all_cooling_total Route attempts where every approved provider key was cooling down.
         # TYPE bluey_provider_key_all_cooling_total counter
         bluey_provider_key_all_cooling_total {provider_key_all_cooling}
         # HELP bluey_provider_health_redis_errors_total Redis read/write errors in the provider-health ledger.
         # TYPE bluey_provider_health_redis_errors_total counter
         bluey_provider_health_redis_errors_total {provider_health_redis_errors}
",
    );

    (StatusCode::OK, body)
}
