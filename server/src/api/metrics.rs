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

use axum::{extract::State, http::StatusCode, response::IntoResponse};

use super::AppState;

pub async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("# pool error: {e}"),
            );
        }
    };

    fn count_one(conn: &rusqlite::Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0)
    }

    let accounts = count_one(&conn, "SELECT COUNT(*) FROM accounts");
    let balance_sum: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(balance_cents), 0) FROM accounts",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let trial_active = count_one(
        &conn,
        "SELECT COUNT(*) FROM accounts WHERE trial_seconds_remaining > 0",
    );
    let req_total = count_one(&conn, "SELECT COUNT(*) FROM request_idempotency");
    let req_complete = count_one(
        &conn,
        "SELECT COUNT(*) FROM request_idempotency WHERE status = 'complete'",
    );
    let req_in_progress = count_one(
        &conn,
        "SELECT COUNT(*) FROM request_idempotency WHERE status = 'in_progress'",
    );
    let mark_complete_failed = count_one(
        &conn,
        "SELECT COUNT(*) FROM request_idempotency
         WHERE status = 'in_progress'
           AND created_at < datetime('now', '-5 minutes')",
    );
    let credit_batches = count_one(&conn, "SELECT COUNT(*) FROM credit_batches");
    let usage_24h = count_one(
        &conn,
        "SELECT COUNT(*) FROM usage_events WHERE ts >= datetime('now', '-1 day')",
    );
    let webhook_processed = count_one(
        &conn,
        "SELECT COUNT(*) FROM stripe_webhook_events WHERE processed_at IS NOT NULL",
    );

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
",
    );

    (StatusCode::OK, body)
}
