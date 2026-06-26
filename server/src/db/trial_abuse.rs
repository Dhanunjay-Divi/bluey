//! Trial grant and abuse ledger.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{config::TrialAbuseConfig, db::DbPool};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrialAbuseSignals {
    pub email_hash: String,
    pub email_domain_hash: Option<String>,
    pub ip_hash: Option<String>,
    pub device_hash: Option<String>,
    pub user_agent_hash: Option<String>,
    pub ip_user_agent_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrialGrantDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrialAbuseEventSummary {
    pub created_at: String,
    pub event_type: String,
    pub severity: i64,
    pub reason: Option<String>,
    pub account_id_hash: Option<String>,
    pub email_hash: Option<String>,
    pub email_domain_hash: Option<String>,
    pub ip_hash: Option<String>,
    pub device_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrialAbuseAdminSummary {
    pub trial_grants_24h: i64,
    pub trial_denials_24h: i64,
    pub abuse_events_24h: i64,
    pub recent_events: Vec<TrialAbuseEventSummary>,
}

impl TrialAbuseSignals {
    pub fn from_raw(
        email: &str,
        ip: Option<&str>,
        device: Option<&str>,
        user_agent: Option<&str>,
    ) -> Self {
        let email_hash = hash_required("email", &email.trim().to_ascii_lowercase());
        let email_domain_hash = email
            .trim()
            .to_ascii_lowercase()
            .rsplit_once('@')
            .and_then(|(_, domain)| clean_optional(domain))
            .map(|domain| hash_required("email_domain", &domain));
        let ip_clean = ip.and_then(clean_optional);
        let device_clean = device.and_then(clean_optional);
        let user_agent_clean = user_agent.and_then(clean_optional);
        let ip_user_agent = match (ip_clean.as_deref(), user_agent_clean.as_deref()) {
            (Some(ip), Some(ua)) => Some(format!("{ip}|{ua}")),
            _ => None,
        };
        Self {
            email_hash,
            email_domain_hash,
            ip_hash: ip_clean.as_deref().map(|value| hash_required("ip", value)),
            device_hash: device_clean
                .as_deref()
                .map(|value| hash_required("device", value)),
            user_agent_hash: user_agent_clean
                .as_deref()
                .map(|value| hash_required("user_agent", value)),
            ip_user_agent_hash: ip_user_agent
                .as_deref()
                .map(|value| hash_required("ip_user_agent", value)),
        }
    }
}

fn clean_optional(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn hash_required(kind: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bluey-trial-abuse-v1:");
    hasher.update(kind.as_bytes());
    hasher.update(b":");
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn evaluate_trial_grant(
    pool: &DbPool,
    cfg: TrialAbuseConfig,
    signals: &TrialAbuseSignals,
) -> Result<TrialGrantDecision> {
    crate::db::run_blocking_db(|| {
        let reason = match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                evaluate_sqlite(&conn, cfg, signals)?
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                evaluate_pg(&mut conn, cfg, signals)?
            }
        };
        if let Some(reason) = reason {
            record_denial(pool, signals, &reason)?;
            Ok(TrialGrantDecision {
                allowed: false,
                reason: Some(reason),
            })
        } else {
            Ok(TrialGrantDecision {
                allowed: true,
                reason: None,
            })
        }
    })
}

pub fn record_grant(
    pool: &DbPool,
    account_id: &str,
    signals: &TrialAbuseSignals,
    granted_seconds: i64,
) -> Result<()> {
    crate::db::run_blocking_db(|| {
        insert_trial_grant(
            pool,
            Some(account_id),
            signals,
            granted_seconds.max(0),
            "granted",
            None,
        )
    })
}

pub fn record_event(
    pool: &DbPool,
    account_id: Option<&str>,
    signals: &TrialAbuseSignals,
    event_type: &str,
    severity: i64,
    reason: Option<&str>,
) -> Result<()> {
    crate::db::run_blocking_db(|| {
        insert_abuse_event(pool, account_id, signals, event_type, severity, reason)
    })
}

pub fn admin_summary(pool: &DbPool, limit: i64) -> Result<TrialAbuseAdminSummary> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let trial_grants_24h = count_one_sqlite(
                &conn,
                "SELECT COUNT(*) FROM trial_grants
                 WHERE decision = 'granted'
                   AND created_at >= datetime('now', '-1 day')",
            )?;
            let trial_denials_24h = count_one_sqlite(
                &conn,
                "SELECT COUNT(*) FROM trial_grants
                 WHERE decision = 'denied'
                   AND created_at >= datetime('now', '-1 day')",
            )?;
            let abuse_events_24h = count_one_sqlite(
                &conn,
                "SELECT COUNT(*) FROM trial_abuse_events
                 WHERE created_at >= datetime('now', '-1 day')",
            )?;
            let mut stmt = conn.prepare(
                "SELECT created_at, event_type, severity, reason, account_id,
                        email_hash, email_domain_hash, ip_hash, device_hash
                 FROM trial_abuse_events
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )?;
            let recent_events = stmt
                .query_map(params![limit.clamp(1, 250)], |row| {
                    let account_id: Option<String> = row.get(4)?;
                    Ok(TrialAbuseEventSummary {
                        created_at: row.get(0)?,
                        event_type: row.get(1)?,
                        severity: row.get(2)?,
                        reason: row.get(3)?,
                        account_id_hash: account_id
                            .as_deref()
                            .map(cue_core::account_id_hash_prefix),
                        email_hash: row.get(5)?,
                        email_domain_hash: row.get(6)?,
                        ip_hash: row.get(7)?,
                        device_hash: row.get(8)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(TrialAbuseAdminSummary {
                trial_grants_24h,
                trial_denials_24h,
                abuse_events_24h,
                recent_events,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let trial_grants_24h = count_one_pg(
                &mut conn,
                "SELECT COUNT(*)::bigint FROM trial_grants
                 WHERE decision = 'granted'
                   AND created_at >= now() - interval '1 day'",
            )?;
            let trial_denials_24h = count_one_pg(
                &mut conn,
                "SELECT COUNT(*)::bigint FROM trial_grants
                 WHERE decision = 'denied'
                   AND created_at >= now() - interval '1 day'",
            )?;
            let abuse_events_24h = count_one_pg(
                &mut conn,
                "SELECT COUNT(*)::bigint FROM trial_abuse_events
                 WHERE created_at >= now() - interval '1 day'",
            )?;
            let rows = conn.query(
                "SELECT created_at, event_type, severity, reason, account_id,
                        email_hash, email_domain_hash, ip_hash, device_hash
                 FROM trial_abuse_events
                 ORDER BY created_at DESC
                 LIMIT $1",
                &[&limit.clamp(1, 250)],
            )?;
            let recent_events = rows
                .into_iter()
                .map(|row| {
                    let created_at: DateTime<Utc> = row.try_get(0)?;
                    let account_id: Option<String> = row.try_get(4)?;
                    Ok(TrialAbuseEventSummary {
                        created_at: created_at.to_rfc3339(),
                        event_type: row.try_get(1)?,
                        severity: row.try_get(2)?,
                        reason: row.try_get(3)?,
                        account_id_hash: account_id
                            .as_deref()
                            .map(cue_core::account_id_hash_prefix),
                        email_hash: row.try_get(5)?,
                        email_domain_hash: row.try_get(6)?,
                        ip_hash: row.try_get(7)?,
                        device_hash: row.try_get(8)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(TrialAbuseAdminSummary {
                trial_grants_24h,
                trial_denials_24h,
                abuse_events_24h,
                recent_events,
            })
        }
    })
}

fn evaluate_sqlite(
    conn: &rusqlite::Connection,
    cfg: TrialAbuseConfig,
    signals: &TrialAbuseSignals,
) -> Result<Option<String>> {
    if count_grants_sqlite(
        conn,
        "email_hash = ?1",
        &[&signals.email_hash as &dyn rusqlite::ToSql],
    )? >= cfg.max_trials_per_email
    {
        return Ok(Some("email_trial_already_used".to_string()));
    }
    if let Some(email_domain_hash) = &signals.email_domain_hash {
        if count_recent_grants_sqlite(
            conn,
            "email_domain_hash = ?1",
            &[email_domain_hash as &dyn rusqlite::ToSql],
        )? >= cfg.max_trials_per_email_domain_per_day
        {
            return Ok(Some("email_domain_trial_velocity".to_string()));
        }
    }
    if let Some(device_hash) = &signals.device_hash {
        if count_grants_sqlite(
            conn,
            "device_hash = ?1",
            &[device_hash as &dyn rusqlite::ToSql],
        )? >= cfg.max_trials_per_device
        {
            return Ok(Some("device_trial_already_used".to_string()));
        }
    }
    if let Some(ip_hash) = &signals.ip_hash {
        if count_recent_grants_sqlite(conn, "ip_hash = ?1", &[ip_hash as &dyn rusqlite::ToSql])?
            >= cfg.max_trials_per_ip_per_day
        {
            return Ok(Some("ip_trial_velocity".to_string()));
        }
    }
    if let Some(ip_user_agent_hash) = &signals.ip_user_agent_hash {
        if count_recent_grants_sqlite(
            conn,
            "ip_user_agent_hash = ?1",
            &[ip_user_agent_hash as &dyn rusqlite::ToSql],
        )? >= cfg.max_trials_per_ip_user_agent_per_day
        {
            return Ok(Some("ip_user_agent_trial_velocity".to_string()));
        }
    }
    Ok(None)
}

fn evaluate_pg(
    conn: &mut postgres::Client,
    cfg: TrialAbuseConfig,
    signals: &TrialAbuseSignals,
) -> Result<Option<String>> {
    if count_grants_pg(
        conn,
        "email_hash = $1",
        &[&signals.email_hash as &(dyn postgres::types::ToSql + Sync)],
    )? >= cfg.max_trials_per_email
    {
        return Ok(Some("email_trial_already_used".to_string()));
    }
    if let Some(email_domain_hash) = &signals.email_domain_hash {
        if count_recent_grants_pg(
            conn,
            "email_domain_hash = $1",
            &[email_domain_hash as &(dyn postgres::types::ToSql + Sync)],
        )? >= cfg.max_trials_per_email_domain_per_day
        {
            return Ok(Some("email_domain_trial_velocity".to_string()));
        }
    }
    if let Some(device_hash) = &signals.device_hash {
        if count_grants_pg(
            conn,
            "device_hash = $1",
            &[device_hash as &(dyn postgres::types::ToSql + Sync)],
        )? >= cfg.max_trials_per_device
        {
            return Ok(Some("device_trial_already_used".to_string()));
        }
    }
    if let Some(ip_hash) = &signals.ip_hash {
        if count_recent_grants_pg(
            conn,
            "ip_hash = $1",
            &[ip_hash as &(dyn postgres::types::ToSql + Sync)],
        )? >= cfg.max_trials_per_ip_per_day
        {
            return Ok(Some("ip_trial_velocity".to_string()));
        }
    }
    if let Some(ip_user_agent_hash) = &signals.ip_user_agent_hash {
        if count_recent_grants_pg(
            conn,
            "ip_user_agent_hash = $1",
            &[ip_user_agent_hash as &(dyn postgres::types::ToSql + Sync)],
        )? >= cfg.max_trials_per_ip_user_agent_per_day
        {
            return Ok(Some("ip_user_agent_trial_velocity".to_string()));
        }
    }
    Ok(None)
}

fn record_denial(pool: &DbPool, signals: &TrialAbuseSignals, reason: &str) -> Result<()> {
    insert_trial_grant(pool, None, signals, 0, "denied", Some(reason))?;
    insert_abuse_event(pool, None, signals, "trial_denied", 2, Some(reason))
}

fn insert_trial_grant(
    pool: &DbPool,
    account_id: Option<&str>,
    signals: &TrialAbuseSignals,
    granted_seconds: i64,
    decision: &str,
    reason: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO trial_grants
                    (id, account_id, email_hash, email_domain_hash, ip_hash, device_hash,
                     user_agent_hash, ip_user_agent_hash, granted_seconds, decision, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    account_id,
                    &signals.email_hash,
                    &signals.email_domain_hash,
                    &signals.ip_hash,
                    &signals.device_hash,
                    &signals.user_agent_hash,
                    &signals.ip_user_agent_hash,
                    granted_seconds,
                    decision,
                    reason
                ],
            )?;
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO trial_grants
                    (id, account_id, email_hash, email_domain_hash, ip_hash, device_hash,
                     user_agent_hash, ip_user_agent_hash, granted_seconds, decision, reason)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &id,
                    &account_id,
                    &signals.email_hash,
                    &signals.email_domain_hash,
                    &signals.ip_hash,
                    &signals.device_hash,
                    &signals.user_agent_hash,
                    &signals.ip_user_agent_hash,
                    &granted_seconds,
                    &decision,
                    &reason,
                ],
            )?;
        }
    }
    Ok(())
}

fn insert_abuse_event(
    pool: &DbPool,
    account_id: Option<&str>,
    signals: &TrialAbuseSignals,
    event_type: &str,
    severity: i64,
    reason: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO trial_abuse_events
                    (id, account_id, email_hash, email_domain_hash, ip_hash, device_hash,
                     user_agent_hash, ip_user_agent_hash, event_type, severity, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    id,
                    account_id,
                    &signals.email_hash,
                    &signals.email_domain_hash,
                    &signals.ip_hash,
                    &signals.device_hash,
                    &signals.user_agent_hash,
                    &signals.ip_user_agent_hash,
                    event_type,
                    severity.max(1),
                    reason
                ],
            )?;
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO trial_abuse_events
                    (id, account_id, email_hash, email_domain_hash, ip_hash, device_hash,
                     user_agent_hash, ip_user_agent_hash, event_type, severity, reason)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &id,
                    &account_id,
                    &signals.email_hash,
                    &signals.email_domain_hash,
                    &signals.ip_hash,
                    &signals.device_hash,
                    &signals.user_agent_hash,
                    &signals.ip_user_agent_hash,
                    &event_type,
                    &severity.max(1),
                    &reason,
                ],
            )?;
        }
    }
    Ok(())
}

fn count_grants_sqlite(
    conn: &rusqlite::Connection,
    predicate: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<i64> {
    let sql =
        format!("SELECT COUNT(*) FROM trial_grants WHERE decision = 'granted' AND {predicate}");
    Ok(conn.query_row(&sql, values, |row| row.get(0))?)
}

fn count_recent_grants_sqlite(
    conn: &rusqlite::Connection,
    predicate: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM trial_grants
         WHERE decision = 'granted'
           AND created_at >= datetime('now', '-1 day')
           AND {predicate}"
    );
    Ok(conn.query_row(&sql, values, |row| row.get(0))?)
}

fn count_grants_pg(
    conn: &mut postgres::Client,
    predicate: &str,
    values: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<i64> {
    let sql = format!(
        "SELECT COUNT(*)::bigint FROM trial_grants WHERE decision = 'granted' AND {predicate}"
    );
    Ok(conn.query_one(&sql, values)?.try_get(0)?)
}

fn count_recent_grants_pg(
    conn: &mut postgres::Client,
    predicate: &str,
    values: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<i64> {
    let since = Utc::now() - Duration::days(1);
    let sql = format!(
        "SELECT COUNT(*)::bigint FROM trial_grants
         WHERE decision = 'granted'
           AND created_at >= $2
           AND {predicate}"
    );
    let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> = values.to_vec();
    params.push(&since);
    Ok(conn.query_one(&sql, &params)?.try_get(0)?)
}

fn count_one_sqlite(conn: &rusqlite::Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn count_one_pg(conn: &mut postgres::Client, sql: &str) -> Result<i64> {
    Ok(conn.query_one(sql, &[])?.try_get(0)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-trial-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn denies_second_trial_for_same_device() {
        let pool = pool();
        let cfg = TrialAbuseConfig::default();
        let signals = TrialAbuseSignals::from_raw(
            "a@example.com",
            Some("198.51.100.10"),
            Some("device-1"),
            Some("ua"),
        );
        assert!(evaluate_trial_grant(&pool, cfg, &signals).unwrap().allowed);
        let account_id = crate::db::accounts::Account::create(&pool, "a@example.com", "stub")
            .unwrap()
            .id;
        record_grant(&pool, &account_id, &signals, 600).unwrap();

        let second = TrialAbuseSignals::from_raw(
            "b@example.com",
            Some("198.51.100.11"),
            Some("device-1"),
            Some("ua"),
        );
        let decision = evaluate_trial_grant(&pool, cfg, &second).unwrap();
        assert!(!decision.allowed);
        assert_eq!(
            decision.reason.as_deref(),
            Some("device_trial_already_used")
        );
    }

    #[test]
    fn denies_domain_velocity_after_limit() {
        let pool = pool();
        let cfg = TrialAbuseConfig {
            max_trials_per_email: 10,
            max_trials_per_email_domain_per_day: 1,
            max_trials_per_device: 10,
            max_trials_per_ip_per_day: 10,
            max_trials_per_ip_user_agent_per_day: 10,
        };
        let first = TrialAbuseSignals::from_raw(
            "a@example.com",
            Some("198.51.100.10"),
            Some("device-1"),
            Some("ua-1"),
        );
        assert!(evaluate_trial_grant(&pool, cfg, &first).unwrap().allowed);
        let account_id = crate::db::accounts::Account::create(&pool, "a@example.com", "stub")
            .unwrap()
            .id;
        record_grant(&pool, &account_id, &first, 600).unwrap();

        let second = TrialAbuseSignals::from_raw(
            "b@example.com",
            Some("198.51.100.11"),
            Some("device-2"),
            Some("ua-2"),
        );
        let decision = evaluate_trial_grant(&pool, cfg, &second).unwrap();
        assert!(!decision.allowed);
        assert_eq!(
            decision.reason.as_deref(),
            Some("email_domain_trial_velocity")
        );
    }
}
