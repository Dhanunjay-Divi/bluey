//! Metrics read model.

use anyhow::Result;

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalHoldMetric {
    pub capability: String,
    pub scope_kind: String,
    pub active_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub accounts: i64,
    pub balance_sum: i64,
    pub trial_active: i64,
    pub request_idempotency_total: i64,
    pub request_idempotency_complete: i64,
    pub request_idempotency_in_progress: i64,
    pub mark_complete_failed: i64,
    pub credit_batches: i64,
    pub usage_24h: i64,
    pub webhook_processed: i64,
    /// Active operational holds grouped only by closed, low-cardinality
    /// capability and scope-kind dimensions. Scope identifiers and reasons
    /// never cross this read-model boundary.
    pub operational_holds: Vec<OperationalHoldMetric>,
    pub paused_discovery_sources: i64,
    pub open_ats_circuits: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobsReadinessSnapshot {
    pub operational_holds: Vec<OperationalHoldMetric>,
    pub paused_discovery_sources: i64,
    pub open_ats_circuits: i64,
}

pub fn snapshot(pool: &DbPool) -> Result<MetricsSnapshot> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(MetricsSnapshot {
                accounts: count_one_sqlite(&conn, "SELECT COUNT(*) FROM accounts")?,
                balance_sum: count_one_sqlite(
                    &conn,
                    "SELECT COALESCE(SUM(balance_cents), 0) FROM accounts",
                )?,
                trial_active: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM accounts WHERE trial_seconds_remaining > 0",
                )?,
                request_idempotency_total: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM request_idempotency",
                )?,
                request_idempotency_complete: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM request_idempotency WHERE status = 'complete'",
                )?,
                request_idempotency_in_progress: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM request_idempotency WHERE status = 'in_progress'",
                )?,
                mark_complete_failed: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM request_idempotency
                     WHERE status = 'in_progress'
                       AND created_at < datetime('now', '-5 minutes')",
                )?,
                credit_batches: count_one_sqlite(&conn, "SELECT COUNT(*) FROM credit_batches")?,
                usage_24h: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM usage_events WHERE origin = 'server' AND ts >= datetime('now', '-1 day')",
                )?,
                webhook_processed: count_one_sqlite(
                    &conn,
                    "SELECT COUNT(*) FROM stripe_webhook_events WHERE processed_at IS NOT NULL",
                )?,
                operational_holds: operational_hold_metrics_sqlite(&conn)?,
                paused_discovery_sources: paused_discovery_sources_sqlite(&conn)?,
                open_ats_circuits: open_ats_circuits_sqlite(&conn)?,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(MetricsSnapshot {
                accounts: count_one_pg(&mut conn, "SELECT COUNT(*)::bigint FROM accounts")?,
                balance_sum: count_one_pg(
                    &mut conn,
                    "SELECT COALESCE(SUM(balance_cents), 0)::bigint FROM accounts",
                )?,
                trial_active: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM accounts WHERE trial_seconds_remaining > 0",
                )?,
                request_idempotency_total: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM request_idempotency",
                )?,
                request_idempotency_complete: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM request_idempotency WHERE status = 'complete'",
                )?,
                request_idempotency_in_progress: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM request_idempotency WHERE status = 'in_progress'",
                )?,
                mark_complete_failed: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM request_idempotency
                     WHERE status = 'in_progress'
                       AND created_at < now() - interval '5 minutes'",
                )?,
                credit_batches: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM credit_batches",
                )?,
                usage_24h: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM usage_events WHERE origin = 'server' AND ts >= now() - interval '1 day'",
                )?,
                webhook_processed: count_one_pg(
                    &mut conn,
                    "SELECT COUNT(*)::bigint FROM stripe_webhook_events WHERE processed_at IS NOT NULL",
                )?,
                operational_holds: operational_hold_metrics_pg(&mut conn)?,
                paused_discovery_sources: paused_discovery_sources_pg(&mut conn)?,
                open_ats_circuits: open_ats_circuits_pg(&mut conn)?,
            })
        }
    })
}

/// Privacy-safe inputs for the Jobs readiness endpoint.
pub fn jobs_readiness_snapshot(pool: &DbPool) -> Result<JobsReadinessSnapshot> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(JobsReadinessSnapshot {
                operational_holds: operational_hold_metrics_sqlite(&conn)?,
                paused_discovery_sources: paused_discovery_sources_sqlite(&conn)?,
                open_ats_circuits: open_ats_circuits_sqlite(&conn)?,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(JobsReadinessSnapshot {
                operational_holds: operational_hold_metrics_pg(&mut conn)?,
                paused_discovery_sources: paused_discovery_sources_pg(&mut conn)?,
                open_ats_circuits: open_ats_circuits_pg(&mut conn)?,
            })
        }
    })
}

fn count_one_sqlite(conn: &rusqlite::Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?)
}

fn count_one_pg(conn: &mut postgres::Client, sql: &str) -> Result<i64> {
    Ok(conn.query_one(sql, &[])?.try_get(0)?)
}

fn operational_hold_metrics_sqlite(
    conn: &rusqlite::Connection,
) -> Result<Vec<OperationalHoldMetric>> {
    operational_hold_metrics_from_counts(
        crate::db::jobs::validated_operational_hold_active_counts_sqlite(conn)
            .map_err(anyhow::Error::new)?,
    )
}

fn operational_hold_metrics_pg(conn: &mut postgres::Client) -> Result<Vec<OperationalHoldMetric>> {
    operational_hold_metrics_from_counts(
        crate::db::jobs::validated_operational_hold_active_counts_postgres(conn)
            .map_err(anyhow::Error::new)?,
    )
}

fn operational_hold_metrics_from_counts(
    counts: Vec<(String, String, i64)>,
) -> Result<Vec<OperationalHoldMetric>> {
    counts
        .into_iter()
        .map(|(capability, scope_kind, active_count)| {
            if active_count < 0 {
                anyhow::bail!("invalid operational hold count")
            }
            Ok(OperationalHoldMetric {
                capability,
                scope_kind,
                active_count,
            })
        })
        .collect()
}

fn paused_discovery_sources_sqlite(conn: &rusqlite::Connection) -> Result<i64> {
    count_one_sqlite(
        conn,
        "SELECT
           (SELECT COUNT(*) FROM jobs_discovery_sources
             WHERE status = 'paused' OR health = 'paused')
           +
           (SELECT COUNT(*) FROM jobs_global_discovery_sources
             WHERE status = 'paused' OR health = 'paused')",
    )
}

fn paused_discovery_sources_pg(conn: &mut postgres::Client) -> Result<i64> {
    count_one_pg(
        conn,
        "SELECT
           ((SELECT COUNT(*) FROM jobs_discovery_sources
              WHERE status = 'paused' OR health = 'paused')
            +
            (SELECT COUNT(*) FROM jobs_global_discovery_sources
              WHERE status = 'paused' OR health = 'paused'))::bigint",
    )
}

fn open_ats_circuits_sqlite(conn: &rusqlite::Connection) -> Result<i64> {
    count_one_sqlite(
        conn,
        "SELECT COUNT(*) FROM jobs_ats_certification_circuit_heads
         WHERE state <> 'closed'",
    )
}

fn open_ats_circuits_pg(conn: &mut postgres::Client) -> Result<i64> {
    count_one_pg(
        conn,
        "SELECT COUNT(*)::bigint FROM jobs_ats_certification_circuit_heads
         WHERE state <> 'closed'",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_hold_metric_projection_groups_without_private_scope_ids() {
        let metrics = operational_hold_metrics_from_counts(vec![
            ("all".to_string(), "global".to_string(), 1),
            ("generation".to_string(), "account".to_string(), 2),
        ])
        .unwrap();
        assert_eq!(
            metrics,
            vec![
                OperationalHoldMetric {
                    capability: "all".to_string(),
                    scope_kind: "global".to_string(),
                    active_count: 1,
                },
                OperationalHoldMetric {
                    capability: "generation".to_string(),
                    scope_kind: "account".to_string(),
                    active_count: 2,
                },
            ]
        );
        let encoded = format!("{metrics:?}");
        for private_scope_id in [
            "private-account-a",
            "private-account-b",
            "private-account-c",
            "private-global-scope",
        ] {
            assert!(!encoded.contains(private_scope_id));
        }
    }

    #[test]
    fn native_jobs_blocker_projection_counts_account_global_and_ats_authorities() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE jobs_discovery_sources (
                status TEXT NOT NULL,
                health TEXT NOT NULL
             );
             CREATE TABLE jobs_global_discovery_sources (
                status TEXT NOT NULL,
                health TEXT NOT NULL
             );
             CREATE TABLE jobs_ats_certification_circuit_heads (
                state TEXT NOT NULL
             );
             INSERT INTO jobs_discovery_sources VALUES
                ('paused', 'healthy'),
                ('active', 'paused'),
                ('active', 'healthy');
             INSERT INTO jobs_global_discovery_sources VALUES
                ('paused', 'healthy'),
                ('active', 'healthy');
             INSERT INTO jobs_ats_certification_circuit_heads VALUES
                ('opened'),
                ('held'),
                ('closed');",
        )
        .unwrap();

        assert_eq!(paused_discovery_sources_sqlite(&conn).unwrap(), 3);
        assert_eq!(open_ats_circuits_sqlite(&conn).unwrap(), 2);
    }
}
