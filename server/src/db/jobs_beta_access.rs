//! Durable authority for the bounded Bluey Jobs public beta.
//!
//! All admission paths serialize on the single cohort row. SQLite uses an
//! immediate transaction and PostgreSQL uses `FOR UPDATE`, so reading status
//! and consuming a first-come slot are one atomic operation on both backends.

use anyhow::Result;
use postgres::GenericClient;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::{ops_audit, DbPool};

pub const PUBLIC_BETA_COHORT_ID: &str = "public-v1";
pub const MAX_PUBLIC_BETA_HARD_CAP: i64 = 10_000;
pub const MAX_PUBLIC_BETA_WINDOW_MS: i64 = 90 * 24 * 60 * 60 * 1_000;

const SQLITE_NOW_MS: &str = "CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)";
const POSTGRES_NOW_MS: &str = "FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT";

pub type PublicBetaResult<T> = std::result::Result<T, PublicBetaError>;

#[derive(Debug, thiserror::Error)]
pub enum PublicBetaError {
    #[error("public beta revision conflict; current revision is {current_revision}")]
    Conflict { current_revision: i64 },
    #[error("invalid public beta mutation: {0}")]
    Invalid(String),
    #[error("public beta capacity reached")]
    CapacityReached,
    #[error("public beta account not found")]
    AccountNotFound,
    #[error("public beta account must be permanent and email verified")]
    AccountIneligible,
    #[error("public beta account deletion is pending")]
    AccountDeletionPending,
    #[error("public beta administration requires a current administrator")]
    AdminActorRequired,
    #[error("public beta database unavailable")]
    Database(#[source] anyhow::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicBetaAccessReason {
    Admitted,
    Denied,
    Suspended,
    NotOpen,
    WindowClosed,
    CapacityReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBetaAccessDecision {
    pub reason: PublicBetaAccessReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicBetaCohortState {
    Draft,
    Open,
    ClosedToNew,
    Suspended,
}

impl PublicBetaCohortState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::ClosedToNew => "closed_to_new",
            Self::Suspended => "suspended",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "draft" => Ok(Self::Draft),
            "open" => Ok(Self::Open),
            "closed_to_new" => Ok(Self::ClosedToNew),
            "suspended" => Ok(Self::Suspended),
            _ => Err(PublicBetaError::Invalid("unknown_cohort_state".to_string()).into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBetaCohort {
    pub id: String,
    pub state: PublicBetaCohortState,
    pub opens_at_ms: Option<i64>,
    pub closes_at_ms: Option<i64>,
    pub hard_cap: i64,
    pub assigned_count: i64,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBetaCohortUpdate {
    pub expected_revision: i64,
    pub state: PublicBetaCohortState,
    pub opens_at_ms: Option<i64>,
    pub closes_at_ms: Option<i64>,
    pub hard_cap: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBetaOverride {
    pub denied: bool,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBetaAggregateSnapshot {
    pub state: PublicBetaCohortState,
    pub hard_cap: i64,
    pub assigned_count: i64,
    pub live_enrollment_count: i64,
    pub public_enrollment_count: i64,
    pub admin_enrollment_count: i64,
    pub active_denial_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicBetaEnrollmentExport {
    pub cohort_id: String,
    pub source: String,
    pub admitted_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicBetaOverrideExport {
    pub cohort_id: String,
    pub denied: bool,
    pub revision: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PublicBetaAccountExport {
    pub enrollments: Vec<PublicBetaEnrollmentExport>,
    pub overrides: Vec<PublicBetaOverrideExport>,
}

impl PublicBetaAccountExport {
    pub fn is_empty(&self) -> bool {
        self.enrollments.is_empty() && self.overrides.is_empty()
    }
}

struct LoadedCohort {
    cohort: PublicBetaCohort,
    db_now_ms: i64,
}

#[derive(Clone, Copy)]
enum AdminAuditContext<'a> {
    Actor(&'a str),
    #[cfg(test)]
    TestOnlyUnaudited,
}

impl<'a> AdminAuditContext<'a> {
    fn actor(self) -> Option<&'a str> {
        match self {
            Self::Actor(actor) => Some(actor),
            #[cfg(test)]
            Self::TestOnlyUnaudited => None,
        }
    }
}

pub fn evaluate_or_enroll_public_beta(
    pool: &DbPool,
    account_id: &str,
) -> PublicBetaResult<PublicBetaAccessDecision> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => evaluate_or_enroll_sqlite(pool, account_id),
        DbPool::Postgres(_) => evaluate_or_enroll_postgres(pool, account_id),
    }))
}

#[cfg(not(test))]
pub(crate) fn public_beta_master_enabled() -> bool {
    std::env::var("BLUEY_JOBS_BETA_ENABLED")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

// Process environment is global, so an environment-backed unit fixture would
// race with the thousands of parallel Jobs tests. Unit tests exercise the
// durable cohort/effect fences with the outer gate deterministically enabled;
// serial HTTP integration tests exercise the real environment-backed kill
// switch through the production implementation above.
#[cfg(test)]
pub(crate) const fn public_beta_master_enabled() -> bool {
    true
}

/// Transaction-local effect authority. This never creates an enrollment.
/// SQLite callers must already hold an `IMMEDIATE` transaction.
pub(crate) fn public_beta_effect_authorized_sqlite_tx(
    conn: &rusqlite::Connection,
    account_id: &str,
) -> Result<bool> {
    if !public_beta_master_enabled() {
        return Ok(false);
    }
    let state = conn
        .query_row(
            "SELECT state FROM jobs_public_beta_cohorts WHERE id = ?1",
            params![PUBLIC_BETA_COHORT_ID],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if !matches!(state.as_deref(), Some("open" | "closed_to_new")) {
        return Ok(false);
    }
    let account = conn
        .query_row(
            "SELECT email_verified_at IS NOT NULL, is_temporary
               FROM accounts WHERE id = ?1",
            params![account_id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0)),
        )
        .optional()?;
    if account != Some((true, false)) {
        return Ok(false);
    }
    let authorized = sqlite_enrollment_exists(conn, account_id)?
        && !load_sqlite_override(conn, account_id)?.denied
        && conn
            .query_row(
                "SELECT 1 FROM account_deletion_intents WHERE account_id = ?1",
                params![account_id],
                |_| Ok(()),
            )
            .optional()?
            .is_none();
    Ok(authorized && public_beta_master_enabled())
}

/// Transaction-local effect authority. This never creates an enrollment.
/// The cohort share lock serializes admission, suspension, and override changes.
pub(crate) fn public_beta_effect_authorized_postgres_tx(
    client: &mut impl GenericClient,
    account_id: &str,
) -> Result<bool> {
    if !public_beta_master_enabled() {
        return Ok(false);
    }
    let cohort = client.query_opt(
        "SELECT state FROM jobs_public_beta_cohorts WHERE id = $1 FOR SHARE",
        &[&PUBLIC_BETA_COHORT_ID],
    )?;
    let Some(cohort) = cohort else {
        return Ok(false);
    };
    let state: String = cohort.try_get(0)?;
    if !matches!(state.as_str(), "open" | "closed_to_new") {
        return Ok(false);
    }
    let account = client.query_opt(
        "SELECT email_verified_at IS NOT NULL, is_temporary
           FROM accounts WHERE id = $1 FOR KEY SHARE",
        &[&account_id],
    )?;
    let Some(account) = account else {
        return Ok(false);
    };
    let verified: bool = account.try_get(0)?;
    let is_temporary: i32 = account.try_get(1)?;
    if !verified || is_temporary != 0 {
        return Ok(false);
    }
    if client
        .query_opt(
            "SELECT 1 FROM account_deletion_intents WHERE account_id = $1",
            &[&account_id],
        )?
        .is_some()
    {
        return Ok(false);
    }
    let authorized = postgres_enrollment_exists(client, account_id)?
        && !load_postgres_override(client, account_id)?.denied;
    Ok(authorized && public_beta_master_enabled())
}

pub fn get_public_beta_cohort(pool: &DbPool) -> PublicBetaResult<PublicBetaCohort> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(load_sqlite_cohort(&conn)?.cohort)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(load_postgres_cohort(&mut **conn, false)?.cohort)
        }
    }))
}

#[cfg(test)]
fn update_public_beta_cohort(
    pool: &DbPool,
    update: PublicBetaCohortUpdate,
) -> PublicBetaResult<PublicBetaCohort> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            update_cohort_sqlite(pool, &update, AdminAuditContext::TestOnlyUnaudited)
        }
        DbPool::Postgres(_) => {
            update_cohort_postgres(pool, &update, AdminAuditContext::TestOnlyUnaudited)
        }
    }))
}

pub fn update_public_beta_cohort_audited(
    pool: &DbPool,
    update: PublicBetaCohortUpdate,
    actor_account_id: &str,
) -> PublicBetaResult<PublicBetaCohort> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            update_cohort_sqlite(pool, &update, AdminAuditContext::Actor(actor_account_id))
        }
        DbPool::Postgres(_) => {
            update_cohort_postgres(pool, &update, AdminAuditContext::Actor(actor_account_id))
        }
    }))
}

#[cfg(test)]
fn grant_public_beta_access(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
) -> PublicBetaResult<PublicBetaAccessDecision> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => grant_access_sqlite(
            pool,
            account_id,
            expected_revision,
            AdminAuditContext::TestOnlyUnaudited,
        ),
        DbPool::Postgres(_) => grant_access_postgres(
            pool,
            account_id,
            expected_revision,
            AdminAuditContext::TestOnlyUnaudited,
        ),
    }))
}

pub fn grant_public_beta_access_audited(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    actor_account_id: &str,
) -> PublicBetaResult<PublicBetaAccessDecision> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => grant_access_sqlite(
            pool,
            account_id,
            expected_revision,
            AdminAuditContext::Actor(actor_account_id),
        ),
        DbPool::Postgres(_) => grant_access_postgres(
            pool,
            account_id,
            expected_revision,
            AdminAuditContext::Actor(actor_account_id),
        ),
    }))
}

pub fn get_public_beta_override(
    pool: &DbPool,
    account_id: &str,
) -> PublicBetaResult<PublicBetaOverride> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            ensure_sqlite_account_exists(&conn, account_id)?;
            load_sqlite_override(&conn, account_id)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            ensure_postgres_account_exists(&mut **conn, account_id, false)?;
            load_postgres_override(&mut **conn, account_id)
        }
    }))
}

#[cfg(test)]
fn set_public_beta_override(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    denied: bool,
) -> PublicBetaResult<PublicBetaOverride> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => set_override_sqlite(
            pool,
            account_id,
            expected_revision,
            denied,
            AdminAuditContext::TestOnlyUnaudited,
        ),
        DbPool::Postgres(_) => set_override_postgres(
            pool,
            account_id,
            expected_revision,
            denied,
            AdminAuditContext::TestOnlyUnaudited,
        ),
    }))
}

pub fn set_public_beta_override_audited(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    denied: bool,
    actor_account_id: &str,
) -> PublicBetaResult<PublicBetaOverride> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => set_override_sqlite(
            pool,
            account_id,
            expected_revision,
            denied,
            AdminAuditContext::Actor(actor_account_id),
        ),
        DbPool::Postgres(_) => set_override_postgres(
            pool,
            account_id,
            expected_revision,
            denied,
            AdminAuditContext::Actor(actor_account_id),
        ),
    }))
}

pub fn public_beta_aggregate_snapshot(
    pool: &DbPool,
) -> PublicBetaResult<PublicBetaAggregateSnapshot> {
    into_public_result(crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => aggregate_snapshot_sqlite(pool),
        DbPool::Postgres(_) => aggregate_snapshot_postgres(pool),
    }))
}

pub(crate) fn public_beta_account_export(
    pool: &DbPool,
    account_id: &str,
) -> Result<PublicBetaAccountExport> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut enrollment_statement = conn.prepare(
                "SELECT cohort_id, source, admitted_at_ms
                   FROM jobs_public_beta_enrollments
                  WHERE account_id = ?1
                  ORDER BY cohort_id",
            )?;
            let enrollments = enrollment_statement
                .query_map(params![account_id], |row| {
                    Ok(PublicBetaEnrollmentExport {
                        cohort_id: row.get(0)?,
                        source: row.get(1)?,
                        admitted_at_ms: row.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut override_statement = conn.prepare(
                "SELECT cohort_id, denied, revision, created_at_ms, updated_at_ms
                   FROM jobs_public_beta_overrides
                  WHERE account_id = ?1
                  ORDER BY cohort_id",
            )?;
            let overrides = override_statement
                .query_map(params![account_id], |row| {
                    Ok(PublicBetaOverrideExport {
                        cohort_id: row.get(0)?,
                        denied: row.get::<_, i64>(1)? != 0,
                        revision: row.get(2)?,
                        created_at_ms: row.get(3)?,
                        updated_at_ms: row.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(PublicBetaAccountExport {
                enrollments,
                overrides,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let enrollments = conn
                .query(
                    "SELECT cohort_id, source, admitted_at_ms
                       FROM jobs_public_beta_enrollments
                      WHERE account_id = $1
                      ORDER BY cohort_id",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| {
                    Ok(PublicBetaEnrollmentExport {
                        cohort_id: row.try_get(0)?,
                        source: row.try_get(1)?,
                        admitted_at_ms: row.try_get(2)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let overrides = conn
                .query(
                    "SELECT cohort_id, denied, revision, created_at_ms, updated_at_ms
                       FROM jobs_public_beta_overrides
                      WHERE account_id = $1
                      ORDER BY cohort_id",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| {
                    Ok(PublicBetaOverrideExport {
                        cohort_id: row.try_get(0)?,
                        denied: row.try_get::<_, i32>(1)? != 0,
                        revision: row.try_get(2)?,
                        created_at_ms: row.try_get(3)?,
                        updated_at_ms: row.try_get(4)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(PublicBetaAccountExport {
                enrollments,
                overrides,
            })
        }
    })
}

fn into_public_result<T>(result: Result<T>) -> PublicBetaResult<T> {
    result.map_err(|error| match error.downcast::<PublicBetaError>() {
        Ok(error) => error,
        Err(error) => PublicBetaError::Database(error),
    })
}

fn decision(reason: PublicBetaAccessReason) -> PublicBetaAccessDecision {
    PublicBetaAccessDecision { reason }
}

fn access_reason_name(reason: PublicBetaAccessReason) -> &'static str {
    match reason {
        PublicBetaAccessReason::Admitted => "admitted",
        PublicBetaAccessReason::Denied => "denied",
        PublicBetaAccessReason::Suspended => "suspended",
        PublicBetaAccessReason::NotOpen => "not_open",
        PublicBetaAccessReason::WindowClosed => "window_closed",
        PublicBetaAccessReason::CapacityReached => "capacity_reached",
    }
}

fn cohort_audit_metadata(cohort: &PublicBetaCohort) -> serde_json::Value {
    json!({
        "state": cohort.state.as_str(),
        "hard_cap": cohort.hard_cap,
        "assigned_count": cohort.assigned_count,
        "revision": cohort.revision
    })
}

fn override_audit_metadata(value: PublicBetaOverride) -> serde_json::Value {
    json!({ "denied": value.denied, "revision": value.revision })
}

fn admin_audit_input(
    actor_account_id: &str,
    target_account_id: Option<&str>,
    event_type: &str,
    metadata_json: serde_json::Value,
) -> ops_audit::OpsAuditEventInput {
    ops_audit::OpsAuditEventInput {
        account_id_hash: target_account_id.map(cue_core::account_id_hash_prefix),
        actor_account_id_hash: Some(cue_core::account_id_hash_prefix(actor_account_id)),
        event_type: event_type.to_string(),
        status: "completed".to_string(),
        metadata_json,
    }
}

fn record_sqlite_admin_audit(
    conn: &rusqlite::Connection,
    actor_account_id: &str,
    target_account_id: Option<&str>,
    event_type: &str,
    metadata_json: serde_json::Value,
) -> Result<()> {
    ops_audit::record_event_sqlite_client(
        conn,
        &admin_audit_input(
            actor_account_id,
            target_account_id,
            event_type,
            metadata_json,
        ),
    )
}

fn record_postgres_admin_audit(
    client: &mut impl GenericClient,
    actor_account_id: &str,
    target_account_id: Option<&str>,
    event_type: &str,
    metadata_json: serde_json::Value,
) -> Result<()> {
    ops_audit::record_event_postgres_client(
        client,
        &admin_audit_input(
            actor_account_id,
            target_account_id,
            event_type,
            metadata_json,
        ),
    )
}

fn evaluate_or_enroll_sqlite(pool: &DbPool, account_id: &str) -> Result<PublicBetaAccessDecision> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let loaded = load_sqlite_cohort(&tx)?;
    ensure_sqlite_account_eligible(&tx, account_id)?;

    let result = if loaded.cohort.state == PublicBetaCohortState::Suspended {
        decision(PublicBetaAccessReason::Suspended)
    } else if load_sqlite_override(&tx, account_id)?.denied {
        decision(PublicBetaAccessReason::Denied)
    } else if sqlite_enrollment_exists(&tx, account_id)? {
        existing_enrollment_decision(loaded.cohort.state)
    } else {
        let reason = new_enrollment_reason(&loaded)?;
        if reason == PublicBetaAccessReason::Admitted {
            consume_sqlite_slot(&tx, account_id, "public_window")?;
        }
        decision(reason)
    };
    tx.commit()?;
    Ok(result)
}

fn evaluate_or_enroll_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<PublicBetaAccessDecision> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let loaded = load_postgres_cohort(&mut tx, true)?;
    ensure_postgres_account_eligible(&mut tx, account_id)?;

    let result = if loaded.cohort.state == PublicBetaCohortState::Suspended {
        decision(PublicBetaAccessReason::Suspended)
    } else if load_postgres_override(&mut tx, account_id)?.denied {
        decision(PublicBetaAccessReason::Denied)
    } else if postgres_enrollment_exists(&mut tx, account_id)? {
        existing_enrollment_decision(loaded.cohort.state)
    } else {
        let reason = new_enrollment_reason(&loaded)?;
        if reason == PublicBetaAccessReason::Admitted {
            consume_postgres_slot(&mut tx, account_id, "public_window")?;
        }
        decision(reason)
    };
    tx.commit()?;
    Ok(result)
}

fn existing_enrollment_decision(state: PublicBetaCohortState) -> PublicBetaAccessDecision {
    let reason = match state {
        PublicBetaCohortState::Open | PublicBetaCohortState::ClosedToNew => {
            PublicBetaAccessReason::Admitted
        }
        PublicBetaCohortState::Draft => PublicBetaAccessReason::NotOpen,
        PublicBetaCohortState::Suspended => PublicBetaAccessReason::Suspended,
    };
    decision(reason)
}

fn new_enrollment_reason(loaded: &LoadedCohort) -> Result<PublicBetaAccessReason> {
    match loaded.cohort.state {
        PublicBetaCohortState::Draft => Ok(PublicBetaAccessReason::NotOpen),
        PublicBetaCohortState::ClosedToNew => Ok(PublicBetaAccessReason::WindowClosed),
        PublicBetaCohortState::Suspended => Ok(PublicBetaAccessReason::Suspended),
        PublicBetaCohortState::Open => {
            let (Some(opens_at_ms), Some(closes_at_ms)) =
                (loaded.cohort.opens_at_ms, loaded.cohort.closes_at_ms)
            else {
                return Err(PublicBetaError::Invalid("open_window_missing".to_string()).into());
            };
            if loaded.db_now_ms < opens_at_ms {
                Ok(PublicBetaAccessReason::NotOpen)
            } else if loaded.db_now_ms >= closes_at_ms {
                Ok(PublicBetaAccessReason::WindowClosed)
            } else if loaded.cohort.assigned_count >= loaded.cohort.hard_cap {
                Ok(PublicBetaAccessReason::CapacityReached)
            } else {
                Ok(PublicBetaAccessReason::Admitted)
            }
        }
    }
}

fn update_cohort_sqlite(
    pool: &DbPool,
    update: &PublicBetaCohortUpdate,
    audit: AdminAuditContext<'_>,
) -> Result<PublicBetaCohort> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let loaded = load_sqlite_cohort(&tx)?;
    if let Some(actor) = audit.actor() {
        ensure_sqlite_admin_actor(&tx, actor)?;
    }
    require_revision(update.expected_revision, loaded.cohort.revision)?;
    validate_cohort_update(&loaded, update)?;
    if cohort_matches_update(&loaded.cohort, update) {
        if let Some(actor) = audit.actor() {
            record_sqlite_admin_audit(
                &tx,
                actor,
                None,
                "jobs.public_beta.cohort_update",
                cohort_audit_metadata(&loaded.cohort),
            )?;
        }
        tx.commit()?;
        return Ok(loaded.cohort);
    }
    let revision = loaded
        .cohort
        .revision
        .checked_add(1)
        .ok_or_else(|| PublicBetaError::Invalid("revision_overflow".to_string()))?;
    let sql = format!(
        "UPDATE jobs_public_beta_cohorts
            SET state = ?1, opens_at_ms = ?2, closes_at_ms = ?3,
                hard_cap = ?4, revision = ?5, updated_at_ms = {SQLITE_NOW_MS}
          WHERE id = ?6 AND revision = ?7"
    );
    let changed = tx.execute(
        &sql,
        params![
            update.state.as_str(),
            update.opens_at_ms,
            update.closes_at_ms,
            update.hard_cap,
            revision,
            PUBLIC_BETA_COHORT_ID,
            update.expected_revision,
        ],
    )?;
    if changed != 1 {
        return Err(PublicBetaError::Conflict {
            current_revision: loaded.cohort.revision,
        }
        .into());
    }
    let cohort = load_sqlite_cohort(&tx)?.cohort;
    if let Some(actor) = audit.actor() {
        record_sqlite_admin_audit(
            &tx,
            actor,
            None,
            "jobs.public_beta.cohort_update",
            cohort_audit_metadata(&cohort),
        )?;
    }
    tx.commit()?;
    Ok(cohort)
}

fn update_cohort_postgres(
    pool: &DbPool,
    update: &PublicBetaCohortUpdate,
    audit: AdminAuditContext<'_>,
) -> Result<PublicBetaCohort> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let loaded = load_postgres_cohort(&mut tx, true)?;
    if let Some(actor) = audit.actor() {
        ensure_postgres_admin_actor(&mut tx, actor)?;
    }
    require_revision(update.expected_revision, loaded.cohort.revision)?;
    validate_cohort_update(&loaded, update)?;
    if cohort_matches_update(&loaded.cohort, update) {
        if let Some(actor) = audit.actor() {
            record_postgres_admin_audit(
                &mut tx,
                actor,
                None,
                "jobs.public_beta.cohort_update",
                cohort_audit_metadata(&loaded.cohort),
            )?;
        }
        tx.commit()?;
        return Ok(loaded.cohort);
    }
    let revision = loaded
        .cohort
        .revision
        .checked_add(1)
        .ok_or_else(|| PublicBetaError::Invalid("revision_overflow".to_string()))?;
    let sql = format!(
        "UPDATE jobs_public_beta_cohorts
            SET state = $1, opens_at_ms = $2, closes_at_ms = $3,
                hard_cap = $4, revision = $5, updated_at_ms = {POSTGRES_NOW_MS}
          WHERE id = $6 AND revision = $7"
    );
    let changed = tx.execute(
        &sql,
        &[
            &update.state.as_str(),
            &update.opens_at_ms,
            &update.closes_at_ms,
            &update.hard_cap,
            &revision,
            &PUBLIC_BETA_COHORT_ID,
            &update.expected_revision,
        ],
    )?;
    if changed != 1 {
        return Err(PublicBetaError::Conflict {
            current_revision: loaded.cohort.revision,
        }
        .into());
    }
    let cohort = load_postgres_cohort(&mut tx, false)?.cohort;
    if let Some(actor) = audit.actor() {
        record_postgres_admin_audit(
            &mut tx,
            actor,
            None,
            "jobs.public_beta.cohort_update",
            cohort_audit_metadata(&cohort),
        )?;
    }
    tx.commit()?;
    Ok(cohort)
}

fn validate_cohort_update(loaded: &LoadedCohort, update: &PublicBetaCohortUpdate) -> Result<()> {
    if update.hard_cap < loaded.cohort.hard_cap {
        return Err(PublicBetaError::Invalid("hard_cap_must_not_decrease".to_string()).into());
    }
    if update.hard_cap > MAX_PUBLIC_BETA_HARD_CAP {
        return Err(PublicBetaError::Invalid("hard_cap_exceeds_bound".to_string()).into());
    }
    if update.hard_cap < loaded.cohort.assigned_count {
        return Err(PublicBetaError::Invalid("hard_cap_below_assigned_count".to_string()).into());
    }
    match (update.opens_at_ms, update.closes_at_ms) {
        (None, None) => {}
        (Some(opens_at_ms), Some(closes_at_ms)) => {
            let duration = closes_at_ms
                .checked_sub(opens_at_ms)
                .ok_or_else(|| PublicBetaError::Invalid("window_overflow".to_string()))?;
            if opens_at_ms < 0 || duration <= 0 || duration > MAX_PUBLIC_BETA_WINDOW_MS {
                return Err(PublicBetaError::Invalid("window_out_of_bounds".to_string()).into());
            }
        }
        _ => return Err(PublicBetaError::Invalid("window_pair_required".to_string()).into()),
    }
    if update.state == PublicBetaCohortState::Open {
        let (Some(_), Some(closes_at_ms)) = (update.opens_at_ms, update.closes_at_ms) else {
            return Err(PublicBetaError::Invalid("open_window_required".to_string()).into());
        };
        if update.hard_cap <= 0 {
            return Err(PublicBetaError::Invalid("open_cap_required".to_string()).into());
        }
        if closes_at_ms <= loaded.db_now_ms {
            return Err(PublicBetaError::Invalid(
                "open_window_must_be_active_or_future".to_string(),
            )
            .into());
        }
    }
    Ok(())
}

fn cohort_matches_update(cohort: &PublicBetaCohort, update: &PublicBetaCohortUpdate) -> bool {
    cohort.state == update.state
        && cohort.opens_at_ms == update.opens_at_ms
        && cohort.closes_at_ms == update.closes_at_ms
        && cohort.hard_cap == update.hard_cap
}

fn grant_access_sqlite(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    audit: AdminAuditContext<'_>,
) -> Result<PublicBetaAccessDecision> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let loaded = load_sqlite_cohort(&tx)?;
    if let Some(actor) = audit.actor() {
        ensure_sqlite_admin_actor(&tx, actor)?;
    }
    ensure_sqlite_account_eligible(&tx, account_id)?;
    require_revision(expected_revision, loaded.cohort.revision)?;
    if !sqlite_enrollment_exists(&tx, account_id)? {
        if loaded.cohort.assigned_count >= loaded.cohort.hard_cap {
            return Err(PublicBetaError::CapacityReached.into());
        }
        consume_sqlite_slot(&tx, account_id, "admin")?;
    }
    let result = if load_sqlite_override(&tx, account_id)?.denied {
        decision(PublicBetaAccessReason::Denied)
    } else {
        existing_enrollment_decision(loaded.cohort.state)
    };
    if let Some(actor) = audit.actor() {
        record_sqlite_admin_audit(
            &tx,
            actor,
            Some(account_id),
            "jobs.public_beta.account_grant",
            json!({ "result": access_reason_name(result.reason) }),
        )?;
    }
    tx.commit()?;
    Ok(result)
}

fn grant_access_postgres(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    audit: AdminAuditContext<'_>,
) -> Result<PublicBetaAccessDecision> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let loaded = load_postgres_cohort(&mut tx, true)?;
    if let Some(actor) = audit.actor() {
        ensure_postgres_admin_actor(&mut tx, actor)?;
    }
    ensure_postgres_account_eligible(&mut tx, account_id)?;
    require_revision(expected_revision, loaded.cohort.revision)?;
    if !postgres_enrollment_exists(&mut tx, account_id)? {
        if loaded.cohort.assigned_count >= loaded.cohort.hard_cap {
            return Err(PublicBetaError::CapacityReached.into());
        }
        consume_postgres_slot(&mut tx, account_id, "admin")?;
    }
    let result = if load_postgres_override(&mut tx, account_id)?.denied {
        decision(PublicBetaAccessReason::Denied)
    } else {
        existing_enrollment_decision(loaded.cohort.state)
    };
    if let Some(actor) = audit.actor() {
        record_postgres_admin_audit(
            &mut tx,
            actor,
            Some(account_id),
            "jobs.public_beta.account_grant",
            json!({ "result": access_reason_name(result.reason) }),
        )?;
    }
    tx.commit()?;
    Ok(result)
}

fn consume_sqlite_slot(conn: &rusqlite::Connection, account_id: &str, source: &str) -> Result<()> {
    let sql = format!(
        "UPDATE jobs_public_beta_cohorts
            SET assigned_count = assigned_count + 1, updated_at_ms = {SQLITE_NOW_MS}
          WHERE id = ?1 AND assigned_count < hard_cap"
    );
    if conn.execute(&sql, params![PUBLIC_BETA_COHORT_ID])? != 1 {
        return Err(PublicBetaError::CapacityReached.into());
    }
    let sql = format!(
        "INSERT INTO jobs_public_beta_enrollments
            (cohort_id, account_id, source, admitted_at_ms)
         VALUES (?1, ?2, ?3, {SQLITE_NOW_MS})"
    );
    conn.execute(&sql, params![PUBLIC_BETA_COHORT_ID, account_id, source])?;
    Ok(())
}

fn consume_postgres_slot(
    client: &mut impl GenericClient,
    account_id: &str,
    source: &str,
) -> Result<()> {
    let sql = format!(
        "UPDATE jobs_public_beta_cohorts
            SET assigned_count = assigned_count + 1, updated_at_ms = {POSTGRES_NOW_MS}
          WHERE id = $1 AND assigned_count < hard_cap"
    );
    if client.execute(&sql, &[&PUBLIC_BETA_COHORT_ID])? != 1 {
        return Err(PublicBetaError::CapacityReached.into());
    }
    let sql = format!(
        "INSERT INTO jobs_public_beta_enrollments
            (cohort_id, account_id, source, admitted_at_ms)
         VALUES ($1, $2, $3, {POSTGRES_NOW_MS})"
    );
    client.execute(&sql, &[&PUBLIC_BETA_COHORT_ID, &account_id, &source])?;
    Ok(())
}

fn set_override_sqlite(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    denied: bool,
    audit: AdminAuditContext<'_>,
) -> Result<PublicBetaOverride> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    load_sqlite_cohort(&tx)?;
    if let Some(actor) = audit.actor() {
        ensure_sqlite_admin_actor(&tx, actor)?;
    }
    ensure_sqlite_account_writable(&tx, account_id)?;
    let current = load_sqlite_override(&tx, account_id)?;
    require_revision(expected_revision, current.revision)?;
    if current.denied == denied {
        if let Some(actor) = audit.actor() {
            record_sqlite_admin_audit(
                &tx,
                actor,
                Some(account_id),
                "jobs.public_beta.account_override",
                override_audit_metadata(current),
            )?;
        }
        tx.commit()?;
        return Ok(current);
    }
    let revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| PublicBetaError::Invalid("override_revision_overflow".to_string()))?;
    let sql = format!(
        "INSERT INTO jobs_public_beta_overrides
            (cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, {SQLITE_NOW_MS}, {SQLITE_NOW_MS})
         ON CONFLICT(cohort_id, account_id) DO UPDATE SET
            denied = excluded.denied,
            revision = excluded.revision,
            updated_at_ms = {SQLITE_NOW_MS}"
    );
    tx.execute(
        &sql,
        params![
            PUBLIC_BETA_COHORT_ID,
            account_id,
            i64::from(denied),
            revision
        ],
    )?;
    let result = PublicBetaOverride { denied, revision };
    if let Some(actor) = audit.actor() {
        record_sqlite_admin_audit(
            &tx,
            actor,
            Some(account_id),
            "jobs.public_beta.account_override",
            override_audit_metadata(result),
        )?;
    }
    tx.commit()?;
    Ok(result)
}

fn set_override_postgres(
    pool: &DbPool,
    account_id: &str,
    expected_revision: i64,
    denied: bool,
    audit: AdminAuditContext<'_>,
) -> Result<PublicBetaOverride> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    load_postgres_cohort(&mut tx, true)?;
    if let Some(actor) = audit.actor() {
        ensure_postgres_admin_actor(&mut tx, actor)?;
    }
    ensure_postgres_account_writable(&mut tx, account_id)?;
    let current = load_postgres_override(&mut tx, account_id)?;
    require_revision(expected_revision, current.revision)?;
    if current.denied == denied {
        if let Some(actor) = audit.actor() {
            record_postgres_admin_audit(
                &mut tx,
                actor,
                Some(account_id),
                "jobs.public_beta.account_override",
                override_audit_metadata(current),
            )?;
        }
        tx.commit()?;
        return Ok(current);
    }
    let revision = current
        .revision
        .checked_add(1)
        .ok_or_else(|| PublicBetaError::Invalid("override_revision_overflow".to_string()))?;
    let sql = format!(
        "INSERT INTO jobs_public_beta_overrides
            (cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms)
         VALUES ($1, $2, $3, $4, {POSTGRES_NOW_MS}, {POSTGRES_NOW_MS})
         ON CONFLICT(cohort_id, account_id) DO UPDATE SET
            denied = EXCLUDED.denied,
            revision = EXCLUDED.revision,
            updated_at_ms = {POSTGRES_NOW_MS}"
    );
    let denied_value = i32::from(denied);
    tx.execute(
        &sql,
        &[
            &PUBLIC_BETA_COHORT_ID,
            &account_id,
            &denied_value,
            &revision,
        ],
    )?;
    let result = PublicBetaOverride { denied, revision };
    if let Some(actor) = audit.actor() {
        record_postgres_admin_audit(
            &mut tx,
            actor,
            Some(account_id),
            "jobs.public_beta.account_override",
            override_audit_metadata(result),
        )?;
    }
    tx.commit()?;
    Ok(result)
}

fn require_revision(expected_revision: i64, current_revision: i64) -> Result<()> {
    if expected_revision != current_revision {
        return Err(PublicBetaError::Conflict { current_revision }.into());
    }
    Ok(())
}

fn load_sqlite_cohort(conn: &rusqlite::Connection) -> Result<LoadedCohort> {
    let sql = format!(
        "SELECT id, state, opens_at_ms, closes_at_ms, hard_cap,
                assigned_count, revision, {SQLITE_NOW_MS}
           FROM jobs_public_beta_cohorts WHERE id = ?1"
    );
    let row = conn
        .query_row(&sql, params![PUBLIC_BETA_COHORT_ID], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("canonical public beta cohort is missing"))?;
    Ok(LoadedCohort {
        cohort: PublicBetaCohort {
            id: row.0,
            state: PublicBetaCohortState::parse(&row.1)?,
            opens_at_ms: row.2,
            closes_at_ms: row.3,
            hard_cap: row.4,
            assigned_count: row.5,
            revision: row.6,
        },
        db_now_ms: row.7,
    })
}

fn load_postgres_cohort(client: &mut impl GenericClient, for_update: bool) -> Result<LoadedCohort> {
    let lock = if for_update { " FOR UPDATE" } else { "" };
    let sql = format!(
        "SELECT id, state, opens_at_ms, closes_at_ms, hard_cap,
                assigned_count, revision, {POSTGRES_NOW_MS}
           FROM jobs_public_beta_cohorts WHERE id = $1{lock}"
    );
    let row = client
        .query_opt(&sql, &[&PUBLIC_BETA_COHORT_ID])?
        .ok_or_else(|| anyhow::anyhow!("canonical public beta cohort is missing"))?;
    let state: String = row.try_get(1)?;
    Ok(LoadedCohort {
        cohort: PublicBetaCohort {
            id: row.try_get(0)?,
            state: PublicBetaCohortState::parse(&state)?,
            opens_at_ms: row.try_get(2)?,
            closes_at_ms: row.try_get(3)?,
            hard_cap: row.try_get(4)?,
            assigned_count: row.try_get(5)?,
            revision: row.try_get(6)?,
        },
        db_now_ms: row.try_get(7)?,
    })
}

fn ensure_sqlite_account_exists(conn: &rusqlite::Connection, account_id: &str) -> Result<()> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(PublicBetaError::AccountNotFound.into());
    }
    Ok(())
}

fn ensure_sqlite_account_writable(conn: &rusqlite::Connection, account_id: &str) -> Result<()> {
    ensure_sqlite_account_exists(conn, account_id)?;
    ensure_sqlite_no_deletion_intent(conn, account_id)
}

fn ensure_sqlite_account_eligible(conn: &rusqlite::Connection, account_id: &str) -> Result<()> {
    let account = conn
        .query_row(
            "SELECT email_verified_at IS NOT NULL, is_temporary
               FROM accounts WHERE id = ?1",
            params![account_id],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0)),
        )
        .optional()?;
    match account {
        None => Err(PublicBetaError::AccountNotFound.into()),
        Some((true, false)) => ensure_sqlite_no_deletion_intent(conn, account_id),
        Some(_) => Err(PublicBetaError::AccountIneligible.into()),
    }
}

fn ensure_sqlite_admin_actor(conn: &rusqlite::Connection, account_id: &str) -> Result<()> {
    let is_admin = conn
        .query_row(
            "SELECT is_admin FROM accounts WHERE id = ?1",
            params![account_id],
            |row| Ok(row.get::<_, i64>(0)? != 0),
        )
        .optional()?;
    if is_admin != Some(true) {
        return Err(PublicBetaError::AdminActorRequired.into());
    }
    ensure_sqlite_no_deletion_intent(conn, account_id)
}

fn ensure_sqlite_no_deletion_intent(conn: &rusqlite::Connection, account_id: &str) -> Result<()> {
    let deletion_pending = conn
        .query_row(
            "SELECT 1 FROM account_deletion_intents WHERE account_id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if deletion_pending {
        return Err(PublicBetaError::AccountDeletionPending.into());
    }
    Ok(())
}

fn ensure_postgres_account_exists(
    client: &mut impl GenericClient,
    account_id: &str,
    lock: bool,
) -> Result<()> {
    let suffix = if lock { " FOR KEY SHARE" } else { "" };
    let sql = format!("SELECT 1 FROM accounts WHERE id = $1{suffix}");
    if client.query_opt(&sql, &[&account_id])?.is_none() {
        return Err(PublicBetaError::AccountNotFound.into());
    }
    Ok(())
}

fn ensure_postgres_account_writable(
    client: &mut impl GenericClient,
    account_id: &str,
) -> Result<()> {
    if client
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .is_none()
    {
        return Err(PublicBetaError::AccountNotFound.into());
    }
    ensure_postgres_no_deletion_intent(client, account_id)
}

fn ensure_postgres_account_eligible(
    client: &mut impl GenericClient,
    account_id: &str,
) -> Result<()> {
    let row = client.query_opt(
        "SELECT email_verified_at IS NOT NULL, is_temporary
           FROM accounts WHERE id = $1 FOR UPDATE",
        &[&account_id],
    )?;
    let Some(row) = row else {
        return Err(PublicBetaError::AccountNotFound.into());
    };
    let verified: bool = row.try_get(0)?;
    let is_temporary: i32 = row.try_get(1)?;
    if verified && is_temporary == 0 {
        ensure_postgres_no_deletion_intent(client, account_id)
    } else {
        Err(PublicBetaError::AccountIneligible.into())
    }
}

fn ensure_postgres_admin_actor(client: &mut impl GenericClient, account_id: &str) -> Result<()> {
    let row = client.query_opt(
        "SELECT is_admin FROM accounts WHERE id = $1 FOR UPDATE",
        &[&account_id],
    )?;
    let Some(row) = row else {
        return Err(PublicBetaError::AdminActorRequired.into());
    };
    if row.try_get::<_, i32>(0)? == 0 {
        return Err(PublicBetaError::AdminActorRequired.into());
    }
    ensure_postgres_no_deletion_intent(client, account_id)
}

fn ensure_postgres_no_deletion_intent(
    client: &mut impl GenericClient,
    account_id: &str,
) -> Result<()> {
    if client
        .query_opt(
            "SELECT 1 FROM account_deletion_intents WHERE account_id = $1",
            &[&account_id],
        )?
        .is_some()
    {
        return Err(PublicBetaError::AccountDeletionPending.into());
    }
    Ok(())
}

fn sqlite_enrollment_exists(conn: &rusqlite::Connection, account_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM jobs_public_beta_enrollments
              WHERE cohort_id = ?1 AND account_id = ?2",
            params![PUBLIC_BETA_COHORT_ID, account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn postgres_enrollment_exists(client: &mut impl GenericClient, account_id: &str) -> Result<bool> {
    Ok(client
        .query_opt(
            "SELECT 1 FROM jobs_public_beta_enrollments
              WHERE cohort_id = $1 AND account_id = $2",
            &[&PUBLIC_BETA_COHORT_ID, &account_id],
        )?
        .is_some())
}

fn load_sqlite_override(
    conn: &rusqlite::Connection,
    account_id: &str,
) -> Result<PublicBetaOverride> {
    Ok(conn
        .query_row(
            "SELECT denied, revision FROM jobs_public_beta_overrides
              WHERE cohort_id = ?1 AND account_id = ?2",
            params![PUBLIC_BETA_COHORT_ID, account_id],
            |row| {
                Ok(PublicBetaOverride {
                    denied: row.get::<_, i64>(0)? != 0,
                    revision: row.get(1)?,
                })
            },
        )
        .optional()?
        .unwrap_or(PublicBetaOverride {
            denied: false,
            revision: 0,
        }))
}

fn load_postgres_override(
    client: &mut impl GenericClient,
    account_id: &str,
) -> Result<PublicBetaOverride> {
    let row = client.query_opt(
        "SELECT denied, revision FROM jobs_public_beta_overrides
          WHERE cohort_id = $1 AND account_id = $2",
        &[&PUBLIC_BETA_COHORT_ID, &account_id],
    )?;
    let Some(row) = row else {
        return Ok(PublicBetaOverride {
            denied: false,
            revision: 0,
        });
    };
    Ok(PublicBetaOverride {
        denied: row.try_get::<_, i32>(0)? != 0,
        revision: row.try_get(1)?,
    })
}

fn aggregate_snapshot_sqlite(pool: &DbPool) -> Result<PublicBetaAggregateSnapshot> {
    let conn = pool.get()?;
    let row = conn.query_row(
        "SELECT c.state, c.hard_cap, c.assigned_count,
                (SELECT COUNT(*) FROM jobs_public_beta_enrollments e
                  WHERE e.cohort_id = c.id),
                (SELECT COUNT(*) FROM jobs_public_beta_enrollments e
                  WHERE e.cohort_id = c.id AND e.source = 'public_window'),
                (SELECT COUNT(*) FROM jobs_public_beta_enrollments e
                  WHERE e.cohort_id = c.id AND e.source = 'admin'),
                (SELECT COUNT(*) FROM jobs_public_beta_overrides o
                  WHERE o.cohort_id = c.id AND o.denied = 1)
           FROM jobs_public_beta_cohorts c WHERE c.id = ?1",
        params![PUBLIC_BETA_COHORT_ID],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;
    Ok(PublicBetaAggregateSnapshot {
        state: PublicBetaCohortState::parse(&row.0)?,
        hard_cap: row.1,
        assigned_count: row.2,
        live_enrollment_count: row.3,
        public_enrollment_count: row.4,
        admin_enrollment_count: row.5,
        active_denial_count: row.6,
    })
}

fn aggregate_snapshot_postgres(pool: &DbPool) -> Result<PublicBetaAggregateSnapshot> {
    let mut conn = pool.get_pg()?;
    let row = conn.query_one(
        "SELECT c.state, c.hard_cap, c.assigned_count,
                (SELECT COUNT(*) FROM jobs_public_beta_enrollments e
                  WHERE e.cohort_id = c.id),
                (SELECT COUNT(*) FROM jobs_public_beta_enrollments e
                  WHERE e.cohort_id = c.id AND e.source = 'public_window'),
                (SELECT COUNT(*) FROM jobs_public_beta_enrollments e
                  WHERE e.cohort_id = c.id AND e.source = 'admin'),
                (SELECT COUNT(*) FROM jobs_public_beta_overrides o
                  WHERE o.cohort_id = c.id AND o.denied = 1)
           FROM jobs_public_beta_cohorts c WHERE c.id = $1",
        &[&PUBLIC_BETA_COHORT_ID],
    )?;
    let state: String = row.try_get(0)?;
    Ok(PublicBetaAggregateSnapshot {
        state: PublicBetaCohortState::parse(&state)?,
        hard_cap: row.try_get(1)?,
        assigned_count: row.try_get(2)?,
        live_enrollment_count: row.try_get(3)?,
        public_enrollment_count: row.try_get(4)?,
        admin_enrollment_count: row.try_get(5)?,
        active_denial_count: row.try_get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{accounts::Account, open_pool, run_migrations};
    use std::sync::{Arc, Barrier};

    fn sqlite_pool(label: &str) -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-public-beta-{label}-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = open_pool(&path).expect("open public beta SQLite fixture");
        run_migrations(&pool).expect("apply public beta SQLite migrations");
        pool
    }

    fn verified_account(pool: &DbPool, label: &str) -> String {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account = Account::create(
            pool,
            &format!("public-beta-{label}-{suffix}@example.test"),
            "test-password-hash",
        )
        .expect("create public beta account");
        assert_eq!(
            Account::mark_email_verified(pool, &account.id).expect("verify public beta account"),
            1
        );
        account.id
    }

    fn admin_account(pool: &DbPool, label: &str) -> String {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account = Account::create_with_admin(
            pool,
            &format!("public-beta-{label}-{suffix}@example.test"),
            "test-password-hash",
            true,
        )
        .expect("create public beta administrator");
        assert_eq!(
            Account::mark_email_verified(pool, &account.id)
                .expect("verify public beta administrator"),
            1
        );
        account.id
    }

    fn insert_deletion_intent(pool: &DbPool, account_id: &str) {
        crate::db::run_blocking_db(|| -> Result<()> {
            match pool {
                DbPool::Sqlite(_) => {
                    pool.get()?.execute(
                        "INSERT INTO account_deletion_intents (
                        account_id, requested_at_ms, last_checked_at_ms,
                        fresh_upload_cutoff_ms, fresh_in_flight_puts
                     ) VALUES (?1, 1000, 1000, 0, 0)",
                        params![account_id],
                    )?;
                    Ok(())
                }
                DbPool::Postgres(_) => {
                    pool.get_pg()?.execute(
                        "INSERT INTO account_deletion_intents (
                        account_id, requested_at_ms, last_checked_at_ms,
                        fresh_upload_cutoff_ms, fresh_in_flight_puts
                     ) VALUES ($1, 1000, 1000, 0, 0)",
                        &[&account_id],
                    )?;
                    Ok(())
                }
            }
        })
        .expect("insert public beta deletion intent");
    }

    fn open_public_beta(pool: &DbPool, hard_cap: i64) -> PublicBetaCohort {
        let cohort = get_public_beta_cohort(pool).expect("load seeded public beta cohort");
        let now_ms = chrono::Utc::now().timestamp_millis();
        update_public_beta_cohort(
            pool,
            PublicBetaCohortUpdate {
                expected_revision: cohort.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: Some(now_ms - 60_000),
                closes_at_ms: Some(now_ms + 24 * 60 * 60 * 1_000),
                hard_cap,
            },
        )
        .expect("open public beta cohort")
    }

    fn delete_account(pool: &DbPool, account_id: &str) -> usize {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .expect("get SQLite delete connection")
                .execute("DELETE FROM accounts WHERE id = ?1", params![account_id])
                .expect("delete SQLite public beta account"),
            DbPool::Postgres(_) => usize::try_from(
                pool.get_pg()
                    .expect("get PostgreSQL delete connection")
                    .execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
                    .expect("delete PostgreSQL public beta account"),
            )
            .expect("convert PostgreSQL deleted row count"),
        })
    }

    fn assert_reason(
        result: PublicBetaResult<PublicBetaAccessDecision>,
        expected: PublicBetaAccessReason,
    ) {
        assert_eq!(
            result.expect("evaluate public beta access").reason,
            expected
        );
    }

    struct PostgresAuditFailureInjection {
        pool: DbPool,
        function_name: String,
        trigger_name: String,
        installed: bool,
    }

    impl PostgresAuditFailureInjection {
        fn install(pool: &DbPool, actor_account_id: &str) -> Self {
            let suffix = uuid::Uuid::new_v4().simple().to_string();
            let function_name = format!("bt_audit_fail_f_{suffix}");
            let trigger_name = format!("bt_audit_fail_t_{suffix}");
            let actor_hash = cue_core::account_id_hash_prefix(actor_account_id);
            debug_assert!(function_name.len() <= 63);
            debug_assert!(trigger_name.len() <= 63);
            debug_assert!(actor_hash.chars().all(|value| value.is_ascii_hexdigit()));

            crate::db::run_blocking_db(|| -> Result<()> {
                let mut conn = pool.get_pg()?;
                let mut tx = conn.transaction()?;
                tx.batch_execute(&format!(
                    "CREATE FUNCTION public.{function_name}()
                     RETURNS trigger LANGUAGE plpgsql AS $$
                     BEGIN RAISE EXCEPTION 'forced audit failure'; END $$;
                     CREATE TRIGGER {trigger_name}
                     BEFORE INSERT ON ops_audit_events
                     FOR EACH ROW WHEN (
                         NEW.event_type LIKE 'jobs.public_beta.%'
                         AND NEW.actor_account_id_hash = '{actor_hash}'
                     )
                     EXECUTE FUNCTION public.{function_name}();"
                ))?;
                tx.commit()?;
                Ok(())
            })
            .expect("install scoped PostgreSQL public beta audit failure injection");

            Self {
                pool: pool.clone(),
                function_name,
                trigger_name,
                installed: true,
            }
        }

        fn try_cleanup(&mut self) -> Result<()> {
            if !self.installed {
                return Ok(());
            }
            crate::db::run_blocking_db(|| -> Result<()> {
                let mut conn = self.pool.get_pg()?;
                let mut tx = conn.transaction()?;
                tx.batch_execute(&format!(
                    "DROP TRIGGER IF EXISTS {} ON ops_audit_events;
                     DROP FUNCTION IF EXISTS public.{}();",
                    self.trigger_name, self.function_name
                ))?;
                let function_signature = format!("public.{}()", self.function_name);
                let row = tx.query_one(
                    "SELECT to_regprocedure($1) IS NOT NULL,
                            EXISTS(
                                SELECT 1 FROM pg_trigger
                                 WHERE tgname = $2 AND NOT tgisinternal
                            )",
                    &[&function_signature, &self.trigger_name],
                )?;
                let function_exists: bool = row.try_get(0)?;
                let trigger_exists: bool = row.try_get(1)?;
                anyhow::ensure!(
                    !function_exists && !trigger_exists,
                    "scoped PostgreSQL audit failure injection cleanup was incomplete"
                );
                tx.commit()?;
                Ok(())
            })?;
            self.installed = false;
            Ok(())
        }

        fn remove(mut self) -> Result<()> {
            self.try_cleanup()
        }
    }

    impl Drop for PostgresAuditFailureInjection {
        fn drop(&mut self) {
            let _ = self.try_cleanup();
        }
    }

    #[test]
    fn sqlite_migration_is_dark_replay_safe_and_validates_admin_bounds() {
        let pool = sqlite_pool("migration");
        run_migrations(&pool).expect("replay public beta SQLite migrations");

        let cohort = get_public_beta_cohort(&pool).expect("load default public beta cohort");
        assert_eq!(cohort.state, PublicBetaCohortState::Draft);
        assert_eq!(cohort.opens_at_ms, None);
        assert_eq!(cohort.closes_at_ms, None);
        assert_eq!(cohort.hard_cap, 0);
        assert_eq!(cohort.assigned_count, 0);
        assert_eq!(cohort.revision, 1);
        let cohort_rows: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM jobs_public_beta_cohorts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(cohort_rows, 1);

        let missing_window = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: 1,
                state: PublicBetaCohortState::Open,
                opens_at_ms: None,
                closes_at_ms: None,
                hard_cap: 1,
            },
        );
        assert!(matches!(missing_window, Err(PublicBetaError::Invalid(_))));
        let excessive_cap = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: 1,
                state: PublicBetaCohortState::Draft,
                opens_at_ms: None,
                closes_at_ms: None,
                hard_cap: MAX_PUBLIC_BETA_HARD_CAP + 1,
            },
        );
        assert!(matches!(excessive_cap, Err(PublicBetaError::Invalid(_))));
        assert_eq!(
            get_public_beta_cohort(&pool).unwrap().revision,
            1,
            "rejected updates must not mutate the cohort"
        );
    }

    #[test]
    fn sqlite_first_come_admission_is_exact_sticky_and_never_reclaims_capacity() {
        let pool = sqlite_pool("first-come");
        let opened = open_public_beta(&pool, 2);
        let account_ids = (0..10)
            .map(|index| verified_account(&pool, &format!("race-{index}")))
            .collect::<Vec<_>>();
        let barrier = Arc::new(Barrier::new(account_ids.len()));
        let handles = account_ids
            .into_iter()
            .map(|account_id| {
                let pool = pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let result = evaluate_or_enroll_public_beta(&pool, &account_id);
                    (account_id, result)
                })
            })
            .collect::<Vec<_>>();

        let mut admitted = Vec::new();
        for handle in handles {
            let (account_id, result) = handle.join().expect("join admission race");
            match result {
                Ok(decision) if decision.reason == PublicBetaAccessReason::Admitted => {
                    admitted.push(account_id);
                }
                Ok(decision) => {
                    assert_eq!(decision.reason, PublicBetaAccessReason::CapacityReached)
                }
                Err(error) => panic!("unexpected admission race error: {error:#}"),
            }
        }
        assert_eq!(admitted.len(), 2);
        let full = get_public_beta_cohort(&pool).unwrap();
        assert_eq!(full.assigned_count, 2);
        assert_eq!(full.revision, opened.revision);

        let retained = admitted[0].clone();
        let absent_clear = set_public_beta_override(&pool, &retained, 0, false).unwrap();
        assert_eq!(
            absent_clear,
            PublicBetaOverride {
                denied: false,
                revision: 0
            }
        );
        let override_rows: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_public_beta_overrides",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(override_rows, 0, "clearing an absent denial is a no-op");
        assert_eq!(
            set_public_beta_override(&pool, &retained, 0, true).unwrap(),
            PublicBetaOverride {
                denied: true,
                revision: 1
            }
        );
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &retained),
            PublicBetaAccessReason::Denied,
        );
        assert_reason(
            grant_public_beta_access(&pool, &retained, full.revision),
            PublicBetaAccessReason::Denied,
        );
        assert_eq!(
            set_public_beta_override(&pool, &retained, 1, false).unwrap(),
            PublicBetaOverride {
                denied: false,
                revision: 2
            }
        );
        assert_eq!(
            set_public_beta_override(&pool, &retained, 2, false).unwrap(),
            PublicBetaOverride {
                denied: false,
                revision: 2
            },
            "replayed override writes must not consume a revision"
        );

        let suspended = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: full.revision,
                state: PublicBetaCohortState::Suspended,
                opens_at_ms: full.opens_at_ms,
                closes_at_ms: full.closes_at_ms,
                hard_cap: full.hard_cap,
            },
        )
        .unwrap();
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &retained),
            PublicBetaAccessReason::Suspended,
        );
        assert_reason(
            grant_public_beta_access(&pool, &retained, suspended.revision),
            PublicBetaAccessReason::Suspended,
        );
        let closed = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: suspended.revision,
                state: PublicBetaCohortState::ClosedToNew,
                opens_at_ms: suspended.opens_at_ms,
                closes_at_ms: suspended.closes_at_ms,
                hard_cap: suspended.hard_cap,
            },
        )
        .unwrap();
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &retained),
            PublicBetaAccessReason::Admitted,
        );

        assert_eq!(delete_account(&pool, &retained), 1);
        let after_delete = public_beta_aggregate_snapshot(&pool).unwrap();
        assert_eq!(after_delete.assigned_count, 2);
        assert_eq!(after_delete.live_enrollment_count, 1);
        assert_eq!(after_delete.active_denial_count, 0);

        let replacement = verified_account(&pool, "replacement");
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &replacement),
            PublicBetaAccessReason::WindowClosed,
        );
        let reopened_full = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: closed.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: closed.opens_at_ms,
                closes_at_ms: closed.closes_at_ms,
                hard_cap: 2,
            },
        )
        .unwrap();
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &replacement),
            PublicBetaAccessReason::CapacityReached,
        );
        let expanded = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: reopened_full.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: reopened_full.opens_at_ms,
                closes_at_ms: reopened_full.closes_at_ms,
                hard_cap: 3,
            },
        )
        .unwrap();
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &replacement),
            PublicBetaAccessReason::Admitted,
        );
        assert_eq!(get_public_beta_cohort(&pool).unwrap().assigned_count, 3);

        let stale_grant =
            grant_public_beta_access(&pool, &admitted[1], expanded.revision.saturating_sub(1));
        assert!(matches!(
            stale_grant,
            Err(PublicBetaError::Conflict {
                current_revision
            }) if current_revision == expanded.revision
        ));
        let lower_cap = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: expanded.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: expanded.opens_at_ms,
                closes_at_ms: expanded.closes_at_ms,
                hard_cap: 2,
            },
        );
        assert!(matches!(lower_cap, Err(PublicBetaError::Invalid(_))));
    }

    #[test]
    fn sqlite_account_eligibility_and_deletion_race_fail_closed() {
        let pool = sqlite_pool("account-race");
        open_public_beta(&pool, 1);

        let unverified = Account::create(
            &pool,
            "public-beta-unverified@example.test",
            "test-password-hash",
        )
        .unwrap();
        assert!(matches!(
            evaluate_or_enroll_public_beta(&pool, &unverified.id),
            Err(PublicBetaError::AccountIneligible)
        ));
        let temporary = verified_account(&pool, "temporary");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE accounts
                    SET is_temporary = 1,
                        temporary_expires_at = datetime('now', '+1 hour')
                  WHERE id = ?1",
                params![temporary],
            )
            .unwrap();
        assert!(matches!(
            evaluate_or_enroll_public_beta(&pool, &temporary),
            Err(PublicBetaError::AccountIneligible)
        ));
        assert!(matches!(
            evaluate_or_enroll_public_beta(&pool, "missing-account"),
            Err(PublicBetaError::AccountNotFound)
        ));

        let target = verified_account(&pool, "delete-race");
        let barrier = Arc::new(Barrier::new(2));
        let evaluate_pool = pool.clone();
        let evaluate_barrier = barrier.clone();
        let evaluate_target = target.clone();
        let evaluate = std::thread::spawn(move || {
            evaluate_barrier.wait();
            evaluate_or_enroll_public_beta(&evaluate_pool, &evaluate_target)
        });
        let delete_pool = pool.clone();
        let delete_barrier = barrier.clone();
        let delete_target = target.clone();
        let delete = std::thread::spawn(move || {
            delete_barrier.wait();
            delete_account(&delete_pool, &delete_target)
        });
        let result = evaluate.join().expect("join deletion-race admission");
        assert_eq!(delete.join().expect("join deletion-race delete"), 1);
        let slot_consumed = match result {
            Ok(decision) => {
                assert_eq!(decision.reason, PublicBetaAccessReason::Admitted);
                true
            }
            Err(PublicBetaError::AccountNotFound) => false,
            Err(error) => panic!("deletion race failed with an unsafe error: {error:#}"),
        };
        let conn = pool.get().unwrap();
        let target_accounts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE id = ?1",
                params![target],
                |row| row.get(0),
            )
            .unwrap();
        let target_enrollments: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_public_beta_enrollments WHERE account_id = ?1",
                params![target],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);
        assert_eq!(target_accounts, 0);
        assert_eq!(target_enrollments, 0);
        assert_eq!(
            get_public_beta_cohort(&pool).unwrap().assigned_count,
            i64::from(slot_consumed)
        );

        let replacement = verified_account(&pool, "delete-race-replacement");
        let expected = if slot_consumed {
            PublicBetaAccessReason::CapacityReached
        } else {
            PublicBetaAccessReason::Admitted
        };
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &replacement),
            expected,
        );
        assert_eq!(get_public_beta_cohort(&pool).unwrap().assigned_count, 1);
    }

    #[test]
    fn sqlite_deletion_intent_fences_public_enrollment_and_admin_grant() {
        let pool = sqlite_pool("deletion-intent");
        let actor = admin_account(&pool, "deletion-intent-actor");
        let target = verified_account(&pool, "deletion-intent-target");
        let opened = open_public_beta(&pool, 1);
        insert_deletion_intent(&pool, &target);

        assert!(matches!(
            evaluate_or_enroll_public_beta(&pool, &target),
            Err(PublicBetaError::AccountDeletionPending)
        ));
        assert!(matches!(
            grant_public_beta_access_audited(&pool, &target, opened.revision, &actor,),
            Err(PublicBetaError::AccountDeletionPending)
        ));
        assert!(matches!(
            set_public_beta_override_audited(&pool, &target, 0, true, &actor),
            Err(PublicBetaError::AccountDeletionPending)
        ));
        assert_eq!(get_public_beta_cohort(&pool).unwrap().assigned_count, 0);
        assert!(!sqlite_enrollment_exists(&pool.get().unwrap(), &target).unwrap());
        assert_eq!(
            get_public_beta_override(&pool, &target).unwrap().revision,
            0
        );
    }

    #[test]
    fn sqlite_concurrent_deletion_intent_that_wins_blocks_enrollment() {
        let pool = sqlite_pool("concurrent-deletion-intent");
        let target = verified_account(&pool, "concurrent-deletion-intent-target");
        open_public_beta(&pool, 1);

        let mut conn = pool.get().unwrap();
        let deletion = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        deletion
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES (?1, 1000, 1000, 0, 0)",
                params![target],
            )
            .unwrap();

        let evaluate_pool = pool.clone();
        let evaluate_target = target.clone();
        let evaluate = std::thread::spawn(move || {
            evaluate_or_enroll_public_beta(&evaluate_pool, &evaluate_target)
        });
        std::thread::sleep(std::time::Duration::from_millis(25));
        deletion.commit().unwrap();

        assert!(matches!(
            evaluate.join().expect("join deletion-intent admission"),
            Err(PublicBetaError::AccountDeletionPending)
        ));
        assert_eq!(get_public_beta_cohort(&pool).unwrap().assigned_count, 0);
    }

    #[test]
    fn sqlite_audited_mutations_require_a_current_admin_actor() {
        let pool = sqlite_pool("admin-actor");
        let non_admin = verified_account(&pool, "non-admin-actor");
        let admin = admin_account(&pool, "admin-actor");
        let target = verified_account(&pool, "admin-target");
        let opened = open_public_beta(&pool, 2);
        let update = PublicBetaCohortUpdate {
            expected_revision: opened.revision,
            state: opened.state,
            opens_at_ms: opened.opens_at_ms,
            closes_at_ms: opened.closes_at_ms,
            hard_cap: opened.hard_cap,
        };

        assert!(matches!(
            update_public_beta_cohort_audited(&pool, update.clone(), &non_admin),
            Err(PublicBetaError::AdminActorRequired)
        ));
        assert!(matches!(
            update_public_beta_cohort_audited(&pool, update, "missing-admin"),
            Err(PublicBetaError::AdminActorRequired)
        ));
        assert!(matches!(
            grant_public_beta_access_audited(&pool, &target, opened.revision, &non_admin,),
            Err(PublicBetaError::AdminActorRequired)
        ));
        assert!(matches!(
            set_public_beta_override_audited(&pool, &target, 0, true, &non_admin),
            Err(PublicBetaError::AdminActorRequired)
        ));
        assert_reason(
            grant_public_beta_access_audited(&pool, &target, opened.revision, &admin),
            PublicBetaAccessReason::Admitted,
        );

        Account::set_admin(&pool, &admin, false).unwrap();
        assert!(matches!(
            set_public_beta_override_audited(&pool, &target, 0, true, &admin),
            Err(PublicBetaError::AdminActorRequired)
        ));
    }

    #[test]
    fn sqlite_deletion_pending_beta_only_export_is_complete_and_read_only() {
        let pool = sqlite_pool("beta-export");
        let target = verified_account(&pool, "beta-export-target");
        open_public_beta(&pool, 1);
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &target),
            PublicBetaAccessReason::Admitted,
        );
        set_public_beta_override(&pool, &target, 0, true).unwrap();
        insert_deletion_intent(&pool, &target);

        let before = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM jobs_profiles WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_application_identities WHERE account_id = ?1)",
                params![target],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .unwrap();
        assert_eq!(before, (0, 0));

        let export =
            crate::db::jobs::account_export(&pool, &target, "beta-export-target@example.test")
                .unwrap()
                .expect("beta-only account export");
        assert!(export.workspace.is_none());
        assert_eq!(export.public_beta_enrollments.len(), 1);
        assert_eq!(
            export.public_beta_enrollments[0].cohort_id,
            PUBLIC_BETA_COHORT_ID
        );
        assert_eq!(export.public_beta_enrollments[0].source, "public_window");
        assert_eq!(export.public_beta_overrides.len(), 1);
        assert!(export.public_beta_overrides[0].denied);
        assert_eq!(export.public_beta_overrides[0].revision, 1);

        let after = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM jobs_profiles WHERE account_id = ?1),
                    (SELECT COUNT(*) FROM jobs_application_identities WHERE account_id = ?1)",
                params![target],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .unwrap();
        assert_eq!(
            after, before,
            "account export must not synthesize Jobs authority"
        );
        let serialized = serde_json::to_value(export).unwrap();
        assert_eq!(serialized["workspace"], serde_json::Value::Null);
        assert_eq!(
            serialized["public_beta_enrollments"][0]["source"],
            "public_window"
        );
        assert_eq!(serialized["public_beta_overrides"][0]["denied"], true);
    }

    #[test]
    fn postgres_beta_export_queries_are_account_scoped() {
        let source = include_str!("jobs_beta_access.rs");
        assert!(source.contains(
            "FROM jobs_public_beta_enrollments\n                      WHERE account_id = $1"
        ));
        assert!(source.contains(
            "FROM jobs_public_beta_overrides\n                      WHERE account_id = $1"
        ));
        assert!(source.contains("denied: row.try_get::<_, i32>(1)? != 0"));
    }

    #[test]
    fn sqlite_admin_mutations_roll_back_when_atomic_audit_insert_fails() {
        let pool = sqlite_pool("audit-rollback");
        let actor = admin_account(&pool, "audit-actor");
        let target = verified_account(&pool, "audit-target");
        let opened = open_public_beta(&pool, 3);
        pool.get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_public_beta_audit
                 BEFORE INSERT ON ops_audit_events
                 WHEN NEW.event_type LIKE 'jobs.public_beta.%'
                 BEGIN SELECT RAISE(ABORT, 'forced audit failure'); END;",
            )
            .unwrap();

        let update = update_public_beta_cohort_audited(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: opened.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: opened.opens_at_ms,
                closes_at_ms: opened.closes_at_ms,
                hard_cap: 3,
            },
            &actor,
        );
        assert!(matches!(update, Err(PublicBetaError::Database(_))));
        assert_eq!(get_public_beta_cohort(&pool).unwrap(), opened);

        let grant = grant_public_beta_access_audited(&pool, &target, opened.revision, &actor);
        assert!(matches!(grant, Err(PublicBetaError::Database(_))));
        assert_eq!(get_public_beta_cohort(&pool).unwrap().assigned_count, 0);
        assert!(!sqlite_enrollment_exists(&pool.get().unwrap(), &target).unwrap());

        let denied = set_public_beta_override_audited(&pool, &target, 0, true, &actor);
        assert!(matches!(denied, Err(PublicBetaError::Database(_))));
        assert_eq!(
            get_public_beta_override(&pool, &target).unwrap().revision,
            0
        );
    }

    #[test]
    fn sqlite_admin_grant_persists_enrollment_but_returns_effective_denial() {
        let pool = sqlite_pool("grant-truth");
        let target = verified_account(&pool, "grant-truth-target");
        let opened = open_public_beta(&pool, 1);
        set_public_beta_override(&pool, &target, 0, true).unwrap();
        assert_reason(
            grant_public_beta_access(&pool, &target, opened.revision),
            PublicBetaAccessReason::Denied,
        );
        assert!(sqlite_enrollment_exists(&pool.get().unwrap(), &target).unwrap());
        assert_eq!(get_public_beta_cohort(&pool).unwrap().assigned_count, 1);
    }

    fn configured_postgres_pool() -> Option<DbPool> {
        let database_url = std::env::var("BLUEY_TEST_POSTGRES_URL").ok()?;
        let pool = crate::db::open_postgres_pool(&database_url)
            .expect("open configured public beta PostgreSQL pool");
        run_migrations(&pool).expect("apply public beta PostgreSQL migrations");
        run_migrations(&pool).expect("replay public beta PostgreSQL migrations");
        crate::db::run_blocking_db(|| {
            let mut conn = pool.get_pg().expect("read PostgreSQL migration ledger");
            let applied: bool = conn
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM bluey_schema_migrations
                         WHERE version = '038_jobs_public_beta_access.sql'
                     )",
                    &[],
                )
                .expect("query public beta PostgreSQL migration ledger")
                .get(0);
            assert!(applied, "PostgreSQL public beta migration must be ledgered");
        });
        Some(pool)
    }

    fn reset_postgres_public_beta(pool: &DbPool) {
        crate::db::run_blocking_db(|| {
            pool.get_pg()
                .expect("get PostgreSQL public beta reset connection")
                .batch_execute(
                    "BEGIN;
                     DELETE FROM jobs_public_beta_overrides;
                     DELETE FROM jobs_public_beta_enrollments;
                     UPDATE jobs_public_beta_cohorts
                        SET state = 'draft', opens_at_ms = NULL, closes_at_ms = NULL,
                            hard_cap = 0, assigned_count = 0, revision = 1,
                            updated_at_ms = FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::BIGINT
                      WHERE id = 'public-v1';
                     COMMIT;",
                )
                .expect("reset configured PostgreSQL public beta authority");
        });
    }

    #[test]
    #[serial_test::serial]
    fn configured_postgres_beta_only_export_is_complete_and_read_only() {
        let Some(pool) = configured_postgres_pool() else {
            return;
        };
        reset_postgres_public_beta(&pool);
        let target = verified_account(&pool, "postgres-beta-export-target");
        let result = (|| -> Result<()> {
            open_public_beta(&pool, 1);
            let decision = evaluate_or_enroll_public_beta(&pool, &target)?;
            anyhow::ensure!(decision.reason == PublicBetaAccessReason::Admitted);
            set_public_beta_override(&pool, &target, 0, true)?;
            insert_deletion_intent(&pool, &target);

            let before = crate::db::run_blocking_db(|| -> Result<(i64, i64)> {
                let mut conn = pool.get_pg()?;
                let row = conn.query_one(
                    "SELECT
                        (SELECT COUNT(*)::bigint FROM jobs_profiles WHERE account_id = $1),
                        (SELECT COUNT(*)::bigint FROM jobs_application_identities
                          WHERE account_id = $1)",
                    &[&target],
                )?;
                Ok((row.try_get::<_, i64>(0)?, row.try_get::<_, i64>(1)?))
            })?;
            anyhow::ensure!(before == (0, 0));

            let export = crate::db::jobs::account_export(
                &pool,
                &target,
                "postgres-beta-export-target@example.test",
            )?
            .ok_or_else(|| anyhow::anyhow!("beta-only PostgreSQL export is missing"))?;
            anyhow::ensure!(export.workspace.is_none());
            anyhow::ensure!(export.public_beta_enrollments.len() == 1);
            anyhow::ensure!(export.public_beta_enrollments[0].cohort_id == PUBLIC_BETA_COHORT_ID);
            anyhow::ensure!(export.public_beta_enrollments[0].source == "public_window");
            anyhow::ensure!(export.public_beta_overrides.len() == 1);
            anyhow::ensure!(export.public_beta_overrides[0].denied);
            anyhow::ensure!(export.public_beta_overrides[0].revision == 1);

            let after = crate::db::run_blocking_db(|| -> Result<(i64, i64)> {
                let mut conn = pool.get_pg()?;
                let row = conn.query_one(
                    "SELECT
                        (SELECT COUNT(*)::bigint FROM jobs_profiles WHERE account_id = $1),
                        (SELECT COUNT(*)::bigint FROM jobs_application_identities
                          WHERE account_id = $1)",
                    &[&target],
                )?;
                Ok((row.try_get::<_, i64>(0)?, row.try_get::<_, i64>(1)?))
            })?;
            anyhow::ensure!(
                after == before,
                "PostgreSQL export synthesized Jobs authority"
            );
            Ok(())
        })();

        reset_postgres_public_beta(&pool);
        let _ = delete_account(&pool, &target);
        result.expect("exercise PostgreSQL beta-only account export");
    }

    #[test]
    #[serial_test::serial]
    fn configured_postgres_enforces_exact_cap_and_serializes_account_deletion() {
        let Some(pool) = configured_postgres_pool() else {
            return;
        };
        reset_postgres_public_beta(&pool);
        let opened = open_public_beta(&pool, 4);
        let account_ids = (0..12)
            .map(|index| verified_account(&pool, &format!("postgres-race-{index}")))
            .collect::<Vec<_>>();
        let cleanup_account_ids = account_ids.clone();
        let barrier = Arc::new(Barrier::new(account_ids.len()));
        let handles = account_ids
            .into_iter()
            .map(|account_id| {
                let pool = pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let result = evaluate_or_enroll_public_beta(&pool, &account_id);
                    (account_id, result)
                })
            })
            .collect::<Vec<_>>();
        let mut admitted = Vec::new();
        for handle in handles {
            let (account_id, result) = handle.join().expect("join PostgreSQL admission race");
            match result {
                Ok(decision) if decision.reason == PublicBetaAccessReason::Admitted => {
                    admitted.push(account_id);
                }
                Ok(decision) => {
                    assert_eq!(decision.reason, PublicBetaAccessReason::CapacityReached)
                }
                Err(error) => panic!("unexpected PostgreSQL admission race error: {error:#}"),
            }
        }
        assert_eq!(admitted.len(), 4);
        let full = get_public_beta_cohort(&pool).unwrap();
        assert_eq!(full.assigned_count, 4);
        assert_eq!(full.revision, opened.revision);

        assert_eq!(delete_account(&pool, &admitted[0]), 1);
        let after_delete = public_beta_aggregate_snapshot(&pool).unwrap();
        assert_eq!(after_delete.assigned_count, 4);
        assert_eq!(after_delete.live_enrollment_count, 3);

        let expanded = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: full.revision,
                state: PublicBetaCohortState::Open,
                opens_at_ms: full.opens_at_ms,
                closes_at_ms: full.closes_at_ms,
                hard_cap: 5,
            },
        )
        .unwrap();
        let target = verified_account(&pool, "postgres-delete-race");
        let replacement = verified_account(&pool, "postgres-delete-race-replacement");
        let barrier = Arc::new(Barrier::new(2));
        let evaluate_pool = pool.clone();
        let evaluate_barrier = barrier.clone();
        let evaluate_target = target.clone();
        let evaluate = std::thread::spawn(move || {
            evaluate_barrier.wait();
            evaluate_or_enroll_public_beta(&evaluate_pool, &evaluate_target)
        });
        let delete_pool = pool.clone();
        let delete_barrier = barrier.clone();
        let delete_target = target.clone();
        let delete = std::thread::spawn(move || {
            delete_barrier.wait();
            delete_account(&delete_pool, &delete_target)
        });
        let target_result = evaluate
            .join()
            .expect("join PostgreSQL deletion-race admission");
        assert_eq!(
            delete.join().expect("join PostgreSQL deletion-race delete"),
            1
        );
        let slot_consumed = match target_result {
            Ok(decision) => {
                assert_eq!(decision.reason, PublicBetaAccessReason::Admitted);
                true
            }
            Err(PublicBetaError::AccountNotFound) => false,
            Err(error) => panic!("PostgreSQL deletion race failed unsafely: {error:#}"),
        };
        assert_reason(
            evaluate_or_enroll_public_beta(&pool, &replacement),
            if slot_consumed {
                PublicBetaAccessReason::CapacityReached
            } else {
                PublicBetaAccessReason::Admitted
            },
        );
        let final_snapshot = public_beta_aggregate_snapshot(&pool).unwrap();
        assert_eq!(final_snapshot.assigned_count, 5);
        assert_eq!(final_snapshot.live_enrollment_count, 4);
        assert_eq!(
            get_public_beta_cohort(&pool).unwrap().revision,
            expanded.revision
        );

        reset_postgres_public_beta(&pool);
        for account_id in cleanup_account_ids.into_iter().chain([target, replacement]) {
            let _ = delete_account(&pool, &account_id);
        }
    }

    #[test]
    #[serial_test::serial]
    fn configured_postgres_admin_truth_and_atomic_audit_match_sqlite() {
        let Some(pool) = configured_postgres_pool() else {
            return;
        };
        reset_postgres_public_beta(&pool);
        let actor = admin_account(&pool, "postgres-audit-actor");
        let target = verified_account(&pool, "postgres-audit-target");
        let grant_target = verified_account(&pool, "postgres-audit-grant-target");
        let denied_new_target = verified_account(&pool, "postgres-denied-grant-target");
        let opened = open_public_beta(&pool, 3);
        set_public_beta_override(&pool, &denied_new_target, 0, true).unwrap();
        assert_reason(
            grant_public_beta_access(&pool, &denied_new_target, opened.revision),
            PublicBetaAccessReason::Denied,
        );
        assert_reason(
            grant_public_beta_access(&pool, &target, opened.revision),
            PublicBetaAccessReason::Admitted,
        );
        set_public_beta_override(&pool, &target, 0, true).unwrap();
        assert_reason(
            grant_public_beta_access(&pool, &target, opened.revision),
            PublicBetaAccessReason::Denied,
        );
        set_public_beta_override(&pool, &target, 1, false).unwrap();
        let suspended = update_public_beta_cohort(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: opened.revision,
                state: PublicBetaCohortState::Suspended,
                opens_at_ms: opened.opens_at_ms,
                closes_at_ms: opened.closes_at_ms,
                hard_cap: opened.hard_cap,
            },
        )
        .unwrap();
        assert_reason(
            grant_public_beta_access(&pool, &target, suspended.revision),
            PublicBetaAccessReason::Suspended,
        );

        let audit_failure = PostgresAuditFailureInjection::install(&pool, &actor);
        let before = get_public_beta_cohort(&pool).unwrap();
        let update_result = update_public_beta_cohort_audited(
            &pool,
            PublicBetaCohortUpdate {
                expected_revision: before.revision,
                state: PublicBetaCohortState::Suspended,
                opens_at_ms: before.opens_at_ms,
                closes_at_ms: before.closes_at_ms,
                hard_cap: before.hard_cap + 1,
            },
            &actor,
        );
        let override_before = get_public_beta_override(&pool, &target).unwrap();
        let assigned_before = before.assigned_count;
        let grant_result =
            grant_public_beta_access_audited(&pool, &grant_target, before.revision, &actor);
        let override_result = set_public_beta_override_audited(
            &pool,
            &target,
            override_before.revision,
            true,
            &actor,
        );
        audit_failure
            .remove()
            .expect("remove scoped PostgreSQL public beta audit failure injection");
        assert!(matches!(update_result, Err(PublicBetaError::Database(_))));
        assert_eq!(get_public_beta_cohort(&pool).unwrap(), before);
        assert!(matches!(grant_result, Err(PublicBetaError::Database(_))));
        assert_eq!(
            get_public_beta_cohort(&pool).unwrap().assigned_count,
            assigned_before
        );
        crate::db::run_blocking_db(|| {
            let mut conn = pool.get_pg().unwrap();
            assert!(!postgres_enrollment_exists(&mut **conn, &grant_target).unwrap());
        });
        assert!(matches!(override_result, Err(PublicBetaError::Database(_))));
        assert_eq!(
            get_public_beta_override(&pool, &target).unwrap(),
            override_before
        );

        reset_postgres_public_beta(&pool);
        let _ = delete_account(&pool, &actor);
        let _ = delete_account(&pool, &target);
        let _ = delete_account(&pool, &grant_target);
        let _ = delete_account(&pool, &denied_new_target);
    }
}
