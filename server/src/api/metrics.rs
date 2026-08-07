//! Protected Prometheus metrics with closed, privacy-safe Jobs dimensions.

use std::{collections::BTreeMap, fmt::Write};

use anyhow::{bail, Result};
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use super::AppState;
use crate::{
    db::{self, metrics::MetricsSnapshot},
    provider_health::ProviderHealthSnapshot,
};

pub(crate) const OPERATIONAL_CAPABILITIES: [&str; 8] = [
    "all",
    "discovery",
    "generation",
    "application_queue",
    "runner_claim",
    "final_submit",
    "mailbox_sync",
    "communication_dispatch",
];

pub(crate) const READINESS_CAPABILITIES: [&str; 7] = [
    "discovery",
    "generation",
    "application_queue",
    "runner_claim",
    "final_submit",
    "mailbox_sync",
    "communication_dispatch",
];

pub(crate) const OPERATIONAL_SCOPE_KINDS: [&str; 12] = [
    "global",
    "discovery_source",
    "ats_provider",
    "ats_adapter",
    "employer_domain",
    "account",
    "career_track",
    "region",
    "runner_kind",
    "mailbox_provider",
    "model_provider",
    "model",
];

pub async fn get_metrics(State(state): State<AppState>) -> Response {
    let metrics = match db::metrics::snapshot(&state.pool) {
        Ok(metrics) => metrics,
        Err(_) => {
            tracing::error!("admin metrics snapshot unavailable");
            return private_metrics_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "# metrics unavailable\n".to_string(),
            );
        }
    };

    match render_metrics(&metrics, state.provider_health.snapshot()) {
        Ok(body) => private_metrics_response(StatusCode::OK, body),
        Err(_) => {
            tracing::error!("admin metrics contained an invalid closed dimension");
            private_metrics_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "# metrics unavailable\n".to_string(),
            )
        }
    }
}

fn render_metrics(
    metrics: &MetricsSnapshot,
    provider_health: ProviderHealthSnapshot,
) -> Result<String> {
    let mut body = String::new();

    writeln!(body, "# HELP bluey_accounts_total Total customer accounts.")?;
    writeln!(body, "# TYPE bluey_accounts_total gauge")?;
    writeln!(body, "bluey_accounts_total {}", metrics.accounts)?;
    writeln!(
        body,
        "# HELP bluey_balance_cents_sum Sum of all account balance_cents."
    )?;
    writeln!(body, "# TYPE bluey_balance_cents_sum gauge")?;
    writeln!(body, "bluey_balance_cents_sum {}", metrics.balance_sum)?;
    writeln!(
        body,
        "# HELP bluey_trial_active_accounts Accounts with trial_seconds_remaining > 0."
    )?;
    writeln!(body, "# TYPE bluey_trial_active_accounts gauge")?;
    writeln!(body, "bluey_trial_active_accounts {}", metrics.trial_active)?;
    writeln!(
        body,
        "# HELP bluey_request_idempotency_total Total request_idempotency rows."
    )?;
    writeln!(body, "# TYPE bluey_request_idempotency_total counter")?;
    writeln!(
        body,
        "bluey_request_idempotency_total {}",
        metrics.request_idempotency_total
    )?;
    writeln!(
        body,
        "# HELP bluey_request_idempotency_complete Rows in complete state."
    )?;
    writeln!(body, "# TYPE bluey_request_idempotency_complete counter")?;
    writeln!(
        body,
        "bluey_request_idempotency_complete {}",
        metrics.request_idempotency_complete
    )?;
    writeln!(
        body,
        "# HELP bluey_request_idempotency_in_progress Rows still in_progress."
    )?;
    writeln!(body, "# TYPE bluey_request_idempotency_in_progress gauge")?;
    writeln!(
        body,
        "bluey_request_idempotency_in_progress {}",
        metrics.request_idempotency_in_progress
    )?;
    writeln!(
        body,
        "# HELP bluey_mark_complete_failures_estimated In-progress rows older than 5 min."
    )?;
    writeln!(
        body,
        "# TYPE bluey_mark_complete_failures_estimated counter"
    )?;
    writeln!(
        body,
        "bluey_mark_complete_failures_estimated {}",
        metrics.mark_complete_failed
    )?;
    writeln!(
        body,
        "# HELP bluey_credit_batches_total Total credit_batches rows."
    )?;
    writeln!(body, "# TYPE bluey_credit_batches_total counter")?;
    writeln!(
        body,
        "bluey_credit_batches_total {}",
        metrics.credit_batches
    )?;
    writeln!(
        body,
        "# HELP bluey_usage_events_24h Usage events recorded in last 24h."
    )?;
    writeln!(body, "# TYPE bluey_usage_events_24h counter")?;
    writeln!(body, "bluey_usage_events_24h {}", metrics.usage_24h)?;
    writeln!(
        body,
        "# HELP bluey_stripe_webhook_processed Stripe webhook events successfully processed."
    )?;
    writeln!(body, "# TYPE bluey_stripe_webhook_processed counter")?;
    writeln!(
        body,
        "bluey_stripe_webhook_processed {}",
        metrics.webhook_processed
    )?;
    writeln!(
        body,
        "# HELP bluey_provider_key_cooldowns_total Provider key cooldown events."
    )?;
    writeln!(body, "# TYPE bluey_provider_key_cooldowns_total counter")?;
    writeln!(
        body,
        "bluey_provider_key_cooldowns_total {}",
        provider_health.cooldowns_total
    )?;
    writeln!(
        body,
        "# HELP bluey_provider_key_all_cooling_total Routes with all approved keys cooling."
    )?;
    writeln!(body, "# TYPE bluey_provider_key_all_cooling_total counter")?;
    writeln!(
        body,
        "bluey_provider_key_all_cooling_total {}",
        provider_health.all_keys_cooling_total
    )?;
    writeln!(
        body,
        "# HELP bluey_provider_health_redis_errors_total Provider-health Redis failures."
    )?;
    writeln!(
        body,
        "# TYPE bluey_provider_health_redis_errors_total counter"
    )?;
    writeln!(
        body,
        "bluey_provider_health_redis_errors_total {}",
        provider_health.redis_errors_total
    )?;

    append_jobs_operational_metrics(&mut body, metrics)?;
    Ok(body)
}

fn append_jobs_operational_metrics(body: &mut String, metrics: &MetricsSnapshot) -> Result<()> {
    if metrics.paused_discovery_sources < 0 || metrics.open_ats_circuits < 0 {
        bail!("invalid native Jobs blocker count")
    }
    let mut active_counts = BTreeMap::new();
    for metric in &metrics.operational_holds {
        if !OPERATIONAL_CAPABILITIES.contains(&metric.capability.as_str())
            || !OPERATIONAL_SCOPE_KINDS.contains(&metric.scope_kind.as_str())
            || metric.active_count < 0
        {
            bail!("invalid operational hold metric dimension")
        }
        if active_counts
            .insert(
                (metric.capability.as_str(), metric.scope_kind.as_str()),
                metric.active_count,
            )
            .is_some()
        {
            bail!("duplicate operational hold metric dimension")
        }
    }

    writeln!(
        body,
        "# HELP bluey_jobs_operational_holds_active Active Jobs operational safety holds."
    )?;
    writeln!(body, "# TYPE bluey_jobs_operational_holds_active gauge")?;
    for capability in OPERATIONAL_CAPABILITIES {
        for scope_kind in OPERATIONAL_SCOPE_KINDS {
            let value = active_counts
                .get(&(capability, scope_kind))
                .copied()
                .unwrap_or(0);
            writeln!(
                body,
                "bluey_jobs_operational_holds_active{{capability=\"{capability}\",scope_kind=\"{scope_kind}\"}} {value}"
            )?;
        }
    }

    writeln!(
        body,
        "# HELP bluey_jobs_discovery_sources_paused Paused Jobs discovery sources."
    )?;
    writeln!(body, "# TYPE bluey_jobs_discovery_sources_paused gauge")?;
    writeln!(
        body,
        "bluey_jobs_discovery_sources_paused {}",
        metrics.paused_discovery_sources
    )?;
    writeln!(
        body,
        "# HELP bluey_jobs_ats_circuits_open Non-closed ATS certification circuits."
    )?;
    writeln!(body, "# TYPE bluey_jobs_ats_circuits_open gauge")?;
    writeln!(
        body,
        "bluey_jobs_ats_circuits_open {}",
        metrics.open_ats_circuits
    )?;

    writeln!(
        body,
        "# HELP bluey_jobs_capability_ready Whether a Jobs capability has no active safety blocker."
    )?;
    writeln!(body, "# TYPE bluey_jobs_capability_ready gauge")?;
    for capability in READINESS_CAPABILITIES {
        let mut blocker_count = 0_i64;
        for scope_kind in OPERATIONAL_SCOPE_KINDS {
            blocker_count = blocker_count
                .checked_add(
                    active_counts
                        .get(&("all", scope_kind))
                        .copied()
                        .unwrap_or(0),
                )
                .and_then(|value| {
                    value.checked_add(
                        active_counts
                            .get(&(capability, scope_kind))
                            .copied()
                            .unwrap_or(0),
                    )
                })
                .ok_or_else(|| anyhow::anyhow!("operational blocker count overflow"))?;
        }
        blocker_count = blocker_count
            .checked_add(match capability {
                "discovery" => metrics.paused_discovery_sources,
                "final_submit" => metrics.open_ats_circuits,
                _ => 0,
            })
            .ok_or_else(|| anyhow::anyhow!("native blocker count overflow"))?;
        let ready = i64::from(blocker_count == 0);
        writeln!(
            body,
            "bluey_jobs_capability_ready{{capability=\"{capability}\"}} {ready}"
        )?;
    }
    Ok(())
}

fn private_metrics_response(status: StatusCode, body: String) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::PRAGMA, "no-cache"),
            (
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            ),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::metrics::OperationalHoldMetric;

    fn render(snapshot: &MetricsSnapshot) -> Result<String> {
        render_metrics(snapshot, ProviderHealthSnapshot::default())
    }

    #[test]
    fn metrics_response_is_private_and_non_storable() {
        let response = private_metrics_response(StatusCode::OK, "metric 1\n".to_string());
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(response.headers()[header::PRAGMA], "no-cache");
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/plain; version=0.0.4; charset=utf-8"
        );
    }

    #[test]
    fn jobs_metrics_use_only_closed_labels_and_omit_private_sentinels() {
        let snapshot = MetricsSnapshot {
            operational_holds: vec![OperationalHoldMetric {
                capability: "generation".to_string(),
                scope_kind: "account".to_string(),
                active_count: 2,
            }],
            ..MetricsSnapshot::default()
        };
        let body = render(&snapshot).unwrap();

        assert!(body.contains(
            "bluey_jobs_operational_holds_active{capability=\"generation\",scope_kind=\"account\"} 2"
        ));
        assert!(body.contains("bluey_jobs_capability_ready{capability=\"generation\"} 0"));
        assert!(body.contains("bluey_jobs_capability_ready{capability=\"discovery\"} 1"));
        for forbidden in [
            "private-account-id",
            "private.example",
            "private-source-key",
            "https://private.example/job",
            "private-model-name",
            "private-reason",
            "private-sha256",
        ] {
            assert!(!body.contains(forbidden));
        }
    }

    #[test]
    fn invalid_or_private_metric_dimensions_fail_closed() {
        let snapshot = MetricsSnapshot {
            operational_holds: vec![OperationalHoldMetric {
                capability: "private-account-id".to_string(),
                scope_kind: "account".to_string(),
                active_count: 1,
            }],
            ..MetricsSnapshot::default()
        };
        assert!(render(&snapshot).is_err());
    }

    #[test]
    fn global_hold_blocks_every_concrete_capability() {
        let snapshot = MetricsSnapshot {
            operational_holds: vec![OperationalHoldMetric {
                capability: "all".to_string(),
                scope_kind: "global".to_string(),
                active_count: 1,
            }],
            ..MetricsSnapshot::default()
        };
        let body = render(&snapshot).unwrap();
        for capability in READINESS_CAPABILITIES {
            assert!(body.contains(&format!(
                "bluey_jobs_capability_ready{{capability=\"{capability}\"}} 0"
            )));
        }
    }

    #[test]
    fn native_authorities_compose_with_capability_readiness() {
        let snapshot = MetricsSnapshot {
            paused_discovery_sources: 2,
            open_ats_circuits: 3,
            ..MetricsSnapshot::default()
        };
        let body = render(&snapshot).unwrap();
        assert!(body.contains("bluey_jobs_discovery_sources_paused 2"));
        assert!(body.contains("bluey_jobs_ats_circuits_open 3"));
        assert!(body.contains("bluey_jobs_capability_ready{capability=\"discovery\"} 0"));
        assert!(body.contains("bluey_jobs_capability_ready{capability=\"final_submit\"} 0"));
        assert!(body.contains("bluey_jobs_capability_ready{capability=\"generation\"} 1"));
    }
}
