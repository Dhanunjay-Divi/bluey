//! Durable, tenant-scoped AI sessions; no remote-session or payment side effects.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::db::{run_blocking_db, DbPool};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS pinky_ai_bindings (
    subject_key TEXT PRIMARY KEY,
    account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    revoked_at BIGINT,
    created_at BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS pinky_ai_sessions (
    id TEXT PRIMARY KEY,
    subject_key TEXT NOT NULL REFERENCES pinky_ai_bindings(subject_key) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('active', 'closed')),
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    closed_at BIGINT,
    CHECK (expires_at > created_at)
);
CREATE INDEX IF NOT EXISTS idx_pinky_ai_sessions_subject ON pinky_ai_sessions(subject_key);
CREATE TABLE IF NOT EXISTS pinky_ai_entitlements (
    subject_key TEXT PRIMARY KEY REFERENCES pinky_ai_bindings(subject_key) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind = 'synthetic_preprod'),
    state TEXT NOT NULL CHECK (state IN ('active', 'revoked')),
    credit_cents BIGINT NOT NULL CHECK (credit_cents > 0),
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    CHECK (expires_at > created_at)
);
CREATE TABLE IF NOT EXISTS pinky_ai_requests (
    request_id TEXT PRIMARY KEY,
    subject_key TEXT NOT NULL REFERENCES pinky_ai_bindings(subject_key) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES pinky_ai_sessions(id) ON DELETE CASCADE,
    prompt_sha256 TEXT NOT NULL,
    response_mode TEXT NOT NULL CHECK (response_mode IN ('default', 'short', 'star')),
    state TEXT NOT NULL CHECK (state IN (
        'admitted', 'running', 'cancellation_requested', 'finished', 'cancelled', 'failed'
    )),
    accounting_status TEXT NOT NULL CHECK (accounting_status IN ('pending', 'settled')),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    terminal_at BIGINT
);
CREATE INDEX IF NOT EXISTS idx_pinky_ai_requests_session
    ON pinky_ai_requests(subject_key,session_id,state);
CREATE TABLE IF NOT EXISTS pinky_ai_cancel_tombstones (
    request_id TEXT PRIMARY KEY,
    subject_key TEXT NOT NULL REFERENCES pinky_ai_bindings(subject_key) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES pinky_ai_sessions(id) ON DELETE CASCADE,
    created_at BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pinky_ai_cancel_tombstones_session
    ON pinky_ai_cancel_tombstones(subject_key,session_id,created_at);
"#;

const MAX_BINDINGS: i64 = 100;
const MAX_SESSIONS: i64 = 10_000;
const MAX_SESSIONS_PER_BINDING: i64 = 1_000;
const MAX_ACTIVE_PER_BINDING: i64 = 3;
const MAX_CANCEL_TOMBSTONES: i64 = 100_000;
const MAX_CANCEL_TOMBSTONES_PER_SESSION: i64 = 1_000;
const EXTERNAL_PASSWORD_SENTINEL: &str = "!external-pinky-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticEntitlement {
    pub credit_cents: i64,
    pub ttl_seconds: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("integration request denied")]
    Denied,
    #[error("integration preprod capacity reached")]
    Capacity,
    #[error("integration request was cancelled")]
    Cancelled,
}

/// Applied only by the explicitly enabled integration router, never by Jobs startup.
pub fn initialize(pool: &DbPool) -> Result<()> {
    run_blocking_db(|| {
        match pool {
            DbPool::Sqlite(_) => pool.get()?.execute_batch(SCHEMA)?,
            DbPool::Postgres(_) => pool.get_pg()?.batch_execute(SCHEMA)?,
        }
        Ok(())
    })
}

pub fn subject_key(issuer: &str, audience: &str, environment: &str, subject: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"bluey:pinky:subject:v2");
    for value in [issuer, audience, environment, subject] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    hex::encode(hash.finalize())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AiSession {
    pub session_id: String,
    pub state: String,
    /// Server-authoritative AI availability. A live session never implies access.
    pub access: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AiRequestStatus {
    pub session_id: String,
    pub request_id: String,
    pub state: String,
    pub accounting_status: String,
}

fn session(id: &str, state: String, access: &'static str, expires_at: i64, now: i64) -> AiSession {
    AiSession {
        session_id: id.to_owned(),
        state: if state == "active" && expires_at <= now {
            "expired".into()
        } else {
            state
        },
        access: access.into(),
        expires_at,
    }
}

fn sqlite_access(
    conn: &rusqlite::Connection,
    key: &str,
    id: &str,
    now: i64,
) -> Result<&'static str> {
    let account_id = format!("pinky_{key}");
    let email = format!("pinky-{key}@accounts.invalid");
    let context_valid: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pinky_ai_sessions s
         JOIN pinky_ai_bindings b ON b.subject_key=s.subject_key
         JOIN accounts a ON a.id=b.account_id
         WHERE s.id=?1 AND s.subject_key=?2 AND s.state='active' AND s.expires_at>?3
           AND b.revoked_at IS NULL AND b.account_id=?4 AND a.email=?5
           AND a.password_hash=?6 AND a.is_admin=0 AND a.trial_seconds_remaining=0
           AND a.billing_restricted=0 AND a.auto_topup_enabled=0
           AND a.is_temporary=0 AND a.temporary_expires_at IS NULL
           AND a.stripe_customer_id IS NULL AND a.stripe_payment_method_id IS NULL
           AND a.square_customer_id IS NULL AND a.square_card_id IS NULL",
        params![id, key, now, account_id, email, EXTERNAL_PASSWORD_SENTINEL],
        |row| row.get(0),
    )?;
    if context_valid != 1 {
        return Ok("unavailable");
    }
    let entitlement_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pinky_ai_entitlements WHERE subject_key=?1",
        [key],
        |row| row.get(0),
    )?;
    if entitlement_exists == 0 {
        return Ok("not_added");
    }
    let available: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pinky_ai_sessions s
         JOIN pinky_ai_bindings b ON b.subject_key=s.subject_key
         JOIN pinky_ai_entitlements e ON e.subject_key=b.subject_key
         JOIN accounts a ON a.id=b.account_id
         WHERE s.id=?1 AND s.subject_key=?2 AND s.state='active' AND s.expires_at>?3
           AND b.revoked_at IS NULL AND b.account_id=?4 AND a.email=?5
           AND e.kind='synthetic_preprod' AND e.state='active' AND e.expires_at>?3
           AND e.credit_cents>0 AND a.balance_cents>0 AND a.billing_restricted=0
           AND a.password_hash=?6 AND a.is_admin=0 AND a.trial_seconds_remaining=0
           AND a.auto_topup_enabled=0 AND a.is_temporary=0
           AND a.temporary_expires_at IS NULL
           AND a.stripe_customer_id IS NULL AND a.stripe_payment_method_id IS NULL
           AND a.square_customer_id IS NULL AND a.square_card_id IS NULL",
        params![id, key, now, account_id, email, EXTERNAL_PASSWORD_SENTINEL],
        |row| row.get(0),
    )?;
    Ok(if available == 1 {
        "available"
    } else {
        "unavailable"
    })
}

fn postgres_access(
    tx: &mut postgres::Transaction<'_>,
    key: &str,
    id: &str,
    now: i64,
) -> Result<&'static str> {
    let account_id = format!("pinky_{key}");
    let email = format!("pinky-{key}@accounts.invalid");
    let context_valid: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM pinky_ai_sessions s
             JOIN pinky_ai_bindings b ON b.subject_key=s.subject_key
             JOIN accounts a ON a.id=b.account_id
             WHERE s.id=$1 AND s.subject_key=$2 AND s.state='active' AND s.expires_at>$3
               AND b.revoked_at IS NULL AND b.account_id=$4 AND a.email=$5
               AND a.password_hash=$6 AND a.is_admin=0 AND a.trial_seconds_remaining=0
               AND a.billing_restricted=0 AND a.auto_topup_enabled=0
               AND a.is_temporary=0 AND a.temporary_expires_at IS NULL
               AND a.stripe_customer_id IS NULL AND a.stripe_payment_method_id IS NULL
               AND a.square_customer_id IS NULL AND a.square_card_id IS NULL",
            &[
                &id,
                &key,
                &now,
                &account_id,
                &email,
                &EXTERNAL_PASSWORD_SENTINEL,
            ],
        )?
        .get(0);
    if context_valid != 1 {
        return Ok("unavailable");
    }
    let entitlement_exists: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM pinky_ai_entitlements WHERE subject_key=$1",
            &[&key],
        )?
        .get(0);
    if entitlement_exists == 0 {
        return Ok("not_added");
    }
    let available: i64 = tx
        .query_one(
            "SELECT COUNT(*) FROM pinky_ai_sessions s
             JOIN pinky_ai_bindings b ON b.subject_key=s.subject_key
             JOIN pinky_ai_entitlements e ON e.subject_key=b.subject_key
             JOIN accounts a ON a.id=b.account_id
             WHERE s.id=$1 AND s.subject_key=$2 AND s.state='active' AND s.expires_at>$3
               AND b.revoked_at IS NULL AND b.account_id=$4 AND a.email=$5
               AND e.kind='synthetic_preprod' AND e.state='active' AND e.expires_at>$3
               AND e.credit_cents>0 AND a.balance_cents>0 AND a.billing_restricted=0
               AND a.password_hash=$6 AND a.is_admin=0 AND a.trial_seconds_remaining=0
               AND a.auto_topup_enabled=0 AND a.is_temporary=0
               AND a.temporary_expires_at IS NULL
               AND a.stripe_customer_id IS NULL AND a.stripe_payment_method_id IS NULL
               AND a.square_customer_id IS NULL AND a.square_card_id IS NULL",
            &[
                &id,
                &key,
                &now,
                &account_id,
                &email,
                &EXTERNAL_PASSWORD_SENTINEL,
            ],
        )?
        .get(0);
    Ok(if available == 1 {
        "available"
    } else {
        "unavailable"
    })
}

/// Trusted Pinky may provision only a distinct external-only account with zero
/// credits/trial/admin/reload. Email equality never links an existing wallet.
/// Account, binding and session are committed atomically. Closed/expired UUIDs
/// cannot be reopened by replay; a new Start needs a new UUID.
#[cfg(test)]
fn open(pool: &DbPool, key: &str, id: &str, now: i64) -> Result<AiSession> {
    open_with_entitlement(pool, key, id, now, None)
}

pub fn open_with_entitlement(
    pool: &DbPool,
    key: &str,
    id: &str,
    now: i64,
    entitlement: Option<SyntheticEntitlement>,
) -> Result<AiSession> {
    let expiry = now.checked_add(3600).ok_or(StoreError::Denied)?;
    let entitlement_expiry = entitlement
        .map(|value| now.checked_add(value.ttl_seconds).ok_or(StoreError::Denied))
        .transpose()?;
    let account_id = format!("pinky_{key}");
    let email = format!("pinky-{key}@accounts.invalid");
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let binding = tx
                .query_row(
                    "SELECT account_id,revoked_at FROM pinky_ai_bindings WHERE subject_key=?1",
                    [key],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            let bound_account = match binding {
                Some((bound, revoked)) if revoked.is_none() => bound,
                Some(_) => return Err(StoreError::Denied.into()),
                None => {
                    let used: i64 =
                        tx.query_row("SELECT COUNT(*) FROM pinky_ai_bindings", [], |row| {
                            row.get(0)
                        })?;
                    if used >= MAX_BINDINGS {
                        return Err(StoreError::Capacity.into());
                    }
                    let created = tx.execute(
                        "INSERT INTO accounts(id,email,password_hash,balance_cents,trial_seconds_remaining,is_admin,auto_topup_enabled)
                         VALUES (?1,?2,?3,0,0,0,0) ON CONFLICT(id) DO NOTHING",
                        params![account_id, email, EXTERNAL_PASSWORD_SENTINEL],
                    )?;
                    if created != 1 {
                        return Err(StoreError::Denied.into());
                    }
                    tx.execute("INSERT INTO pinky_ai_bindings(subject_key,account_id,created_at) VALUES (?1,?2,?3)",
                        params![key, account_id, now])?;
                    if let (Some(value), Some(expires_at)) = (entitlement, entitlement_expiry) {
                        tx.execute(
                            "INSERT INTO pinky_ai_entitlements
                             (subject_key,kind,state,credit_cents,created_at,expires_at)
                             VALUES (?1,'synthetic_preprod','active',?2,?3,?4)",
                            params![key, value.credit_cents, now, expires_at],
                        )?;
                        let source_id = format!("pinky-preprod:{key}");
                        let batch_expiry = chrono::DateTime::from_timestamp(expires_at, 0)
                            .ok_or(StoreError::Denied)?
                            .to_rfc3339();
                        tx.execute(
                            "INSERT INTO credit_batches
                             (id,account_id,amount_cents,remaining_cents,expires_at,stripe_charge_id)
                             VALUES (?1,?2,?3,?3,?4,?5)",
                            params![
                                uuid::Uuid::new_v4().to_string(),
                                account_id,
                                value.credit_cents,
                                batch_expiry,
                                source_id
                            ],
                        )?;
                        tx.execute(
                            "UPDATE accounts SET balance_cents=?1 WHERE id=?2",
                            params![value.credit_cents, account_id],
                        )?;
                        crate::db::balance::insert_balance_ledger_sqlite_tx(
                            &tx,
                            crate::db::balance::BalanceLedgerEntry {
                                account_id: &account_id,
                                event_type: "synthetic_test_credit",
                                amount_cents: value.credit_cents,
                                balance_cents_before: 0,
                                balance_cents_after: value.credit_cents,
                                reason: Some("pinky_preprod_entitlement"),
                                provider: None,
                                processor_payment_id: None,
                                source_id: Some(&source_id),
                                idempotency_key: Some(&source_id),
                                request_id: None,
                                metadata_json: Some("{\"synthetic_preprod\":true}"),
                            },
                        )?;
                    }
                    account_id.clone()
                }
            };
            if bound_account != account_id {
                return Err(StoreError::Denied.into());
            }
            let valid_origin: i64 = tx.query_row(
                "SELECT COUNT(*) FROM accounts WHERE id=?1 AND email=?2 AND password_hash=?3
                 AND is_admin=0 AND trial_seconds_remaining=0 AND billing_restricted=0
                 AND auto_topup_enabled=0 AND is_temporary=0 AND temporary_expires_at IS NULL
                 AND stripe_customer_id IS NULL AND stripe_payment_method_id IS NULL
                 AND square_customer_id IS NULL AND square_card_id IS NULL",
                params![account_id, email, EXTERNAL_PASSWORD_SENTINEL],
                |row| row.get(0),
            )?;
            if valid_origin != 1 {
                return Err(StoreError::Denied.into());
            }
            let replay = tx
                .query_row(
                    "SELECT subject_key,state,expires_at FROM pinky_ai_sessions WHERE id=?1",
                    [id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((owner, state, expires_at)) = replay {
                if owner != key {
                    return Err(StoreError::Denied.into());
                }
                let access = sqlite_access(&tx, key, id, now)?;
                tx.commit()?;
                return Ok(session(id, state, access, expires_at, now));
            }
            let totals: (i64, i64, i64) = tx.query_row(
                "SELECT (SELECT COUNT(*) FROM pinky_ai_sessions),
                        (SELECT COUNT(*) FROM pinky_ai_sessions WHERE subject_key=?1),
                        (SELECT COUNT(*) FROM pinky_ai_sessions WHERE subject_key=?1 AND state='active' AND expires_at>?2)",
                params![key, now], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            if totals.0 >= MAX_SESSIONS
                || totals.1 >= MAX_SESSIONS_PER_BINDING
                || totals.2 >= MAX_ACTIVE_PER_BINDING
            {
                return Err(StoreError::Capacity.into());
            }
            tx.execute(
                "INSERT INTO pinky_ai_sessions(id,subject_key,state,created_at,expires_at)
                 VALUES (?1,?2,'active',?3,?4)",
                params![id, key, now, expiry],
            )?;
            let access = sqlite_access(&tx, key, id, now)?;
            tx.commit()?;
            Ok(session(id, "active".into(), access, expiry, now))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            // Global integration admission lock: per-subject locks alone would
            // let distinct subjects race past the total durable cardinality cap.
            tx.query_one("SELECT pg_advisory_xact_lock(1112299865,626)", &[])?;
            let existing = tx.query_opt(
                "SELECT account_id,revoked_at FROM pinky_ai_bindings WHERE subject_key=$1 FOR UPDATE",
                &[&key],
            )?;
            if existing.is_none() {
                let bindings: i64 = tx
                    .query_one("SELECT COUNT(*) FROM pinky_ai_bindings", &[])?
                    .get(0);
                if bindings >= MAX_BINDINGS {
                    return Err(StoreError::Capacity.into());
                }
                let created = tx.execute(
                    "INSERT INTO accounts(id,email,password_hash,balance_cents,
                 trial_seconds_remaining,is_admin,auto_topup_enabled)
                 VALUES ($1,$2,$3,0,0,0,0) ON CONFLICT(id) DO NOTHING",
                    &[&account_id, &email, &EXTERNAL_PASSWORD_SENTINEL],
                )?;
                if created != 1 {
                    return Err(StoreError::Denied.into());
                }
                tx.execute(
                    "INSERT INTO pinky_ai_bindings(subject_key,account_id,created_at)
                 VALUES ($1,$2,$3)",
                    &[&key, &account_id, &now],
                )?;
                if let (Some(value), Some(expires_at)) = (entitlement, entitlement_expiry) {
                    tx.execute(
                        "INSERT INTO pinky_ai_entitlements
                         (subject_key,kind,state,credit_cents,created_at,expires_at)
                         VALUES ($1,'synthetic_preprod','active',$2,$3,$4)",
                        &[&key, &value.credit_cents, &now, &expires_at],
                    )?;
                    let source_id = format!("pinky-preprod:{key}");
                    let batch_expiry = chrono::DateTime::from_timestamp(expires_at, 0)
                        .ok_or(StoreError::Denied)?;
                    tx.execute(
                        "INSERT INTO credit_batches
                         (id,account_id,amount_cents,remaining_cents,expires_at,stripe_charge_id)
                         VALUES ($1,$2,$3,$3,$4,$5)",
                        &[
                            &uuid::Uuid::new_v4().to_string(),
                            &account_id,
                            &value.credit_cents,
                            &batch_expiry,
                            &source_id,
                        ],
                    )?;
                    tx.execute(
                        "UPDATE accounts SET balance_cents=$1 WHERE id=$2",
                        &[&value.credit_cents, &account_id],
                    )?;
                    crate::db::balance::insert_balance_ledger_pg_tx(
                        &mut tx,
                        crate::db::balance::BalanceLedgerEntry {
                            account_id: &account_id,
                            event_type: "synthetic_test_credit",
                            amount_cents: value.credit_cents,
                            balance_cents_before: 0,
                            balance_cents_after: value.credit_cents,
                            reason: Some("pinky_preprod_entitlement"),
                            provider: None,
                            processor_payment_id: None,
                            source_id: Some(&source_id),
                            idempotency_key: Some(&source_id),
                            request_id: None,
                            metadata_json: Some("{\"synthetic_preprod\":true}"),
                        },
                    )?;
                }
            }
            let binding = tx.query_one(
                "SELECT account_id,revoked_at FROM pinky_ai_bindings
                 WHERE subject_key=$1 FOR UPDATE",
                &[&key],
            )?;
            if binding.get::<_, String>(0) != account_id
                || binding.get::<_, Option<i64>>(1).is_some()
            {
                return Err(StoreError::Denied.into());
            }
            let valid_origin: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM accounts WHERE id=$1 AND email=$2 AND password_hash=$3
                 AND is_admin=0 AND trial_seconds_remaining=0 AND billing_restricted=0
                 AND auto_topup_enabled=0 AND is_temporary=0 AND temporary_expires_at IS NULL
                 AND stripe_customer_id IS NULL AND stripe_payment_method_id IS NULL
                 AND square_customer_id IS NULL AND square_card_id IS NULL",
                    &[&account_id, &email, &EXTERNAL_PASSWORD_SENTINEL],
                )?
                .get(0);
            if valid_origin != 1 {
                return Err(StoreError::Denied.into());
            }
            if let Some(row) = tx.query_opt(
                "SELECT subject_key,state,expires_at FROM pinky_ai_sessions WHERE id=$1",
                &[&id],
            )? {
                if row.get::<_, String>(0) != key {
                    return Err(StoreError::Denied.into());
                }
                let access = postgres_access(&mut tx, key, id, now)?;
                let result = session(id, row.get(1), access, row.get(2), now);
                tx.commit()?;
                return Ok(result);
            }
            let counts = tx.query_one("SELECT (SELECT COUNT(*) FROM pinky_ai_sessions), (SELECT COUNT(*) FROM pinky_ai_sessions WHERE subject_key=$1), (SELECT COUNT(*) FROM pinky_ai_sessions WHERE subject_key=$1 AND state='active' AND expires_at>$2)", &[&key, &now])?;
            if counts.get::<_, i64>(0) >= MAX_SESSIONS
                || counts.get::<_, i64>(1) >= MAX_SESSIONS_PER_BINDING
                || counts.get::<_, i64>(2) >= MAX_ACTIVE_PER_BINDING
            {
                return Err(StoreError::Capacity.into());
            }
            tx.execute(
                "INSERT INTO pinky_ai_sessions(id,subject_key,state,created_at,expires_at)
                 VALUES ($1,$2,'active',$3,$4)",
                &[&id, &key, &now, &expiry],
            )?;
            let row = tx.query_one(
                "SELECT subject_key,state,expires_at FROM pinky_ai_sessions WHERE id=$1",
                &[&id],
            )?;
            let access = postgres_access(&mut tx, key, id, now)?;
            let result = session(id, row.get(1), access, row.get(2), now);
            tx.commit()?;
            Ok(result)
        }
    })
}

/// An authenticated owner can stop a session even after entitlement is revoked.
pub fn close(pool: &DbPool, key: &str, id: &str, now: i64) -> Result<Option<AiSession>> {
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let tx = conn.unchecked_transaction()?;
            let row = tx
                .query_row(
                    "UPDATE pinky_ai_sessions SET state='closed',closed_at=COALESCE(closed_at,?3)
                     WHERE id=?1 AND subject_key=?2 RETURNING expires_at",
                    params![id, key, now],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if row.is_some() {
                tx.execute(
                    "UPDATE pinky_ai_requests
                     SET state='cancellation_requested',updated_at=?3
                     WHERE subject_key=?1 AND session_id=?2
                       AND state IN ('admitted','running')",
                    params![key, id, now],
                )?;
            }
            tx.commit()?;
            Ok(row.map(|expiry| session(id, "closed".into(), "unavailable", expiry, now)))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "UPDATE pinky_ai_sessions SET state='closed',closed_at=COALESCE(closed_at,$3)
                 WHERE id=$1 AND subject_key=$2 RETURNING expires_at",
                &[&id, &key, &now],
            )?;
            if row.is_some() {
                tx.execute(
                    "UPDATE pinky_ai_requests
                     SET state='cancellation_requested',updated_at=$3
                     WHERE subject_key=$1 AND session_id=$2
                       AND state IN ('admitted','running')",
                    &[&key, &id, &now],
                )?;
            }
            tx.commit()?;
            Ok(row.map(|row| session(id, "closed".into(), "unavailable", row.get(0), now)))
        }
    })
}

fn request_status(
    session_id: &str,
    request_id: &str,
    state: String,
    accounting_status: String,
) -> AiRequestStatus {
    AiRequestStatus {
        session_id: session_id.to_string(),
        request_id: request_id.to_string(),
        state,
        accounting_status,
    }
}

/// Atomically resolves the immutable subject binding and verifies the exact
/// active AI context, explicit synthetic entitlement, and positive bound-account
/// credit. The prompt hash/mode bind request-id replay without storing content.
pub fn admit_ask(
    pool: &DbPool,
    key: &str,
    session_id: &str,
    request_id: &str,
    prompt_sha256: &str,
    response_mode: &str,
    now: i64,
) -> Result<(String, AiRequestStatus)> {
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let tombstone = tx
                .query_row(
                    "SELECT subject_key,session_id FROM pinky_ai_cancel_tombstones
                     WHERE request_id=?1",
                    [request_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if let Some((owner, session)) = tombstone {
                return Err(if owner == key && session == session_id {
                    StoreError::Cancelled.into()
                } else {
                    StoreError::Denied.into()
                });
            }
            let account_id = tx
                .query_row(
                    "SELECT b.account_id
                     FROM pinky_ai_bindings b
                     JOIN pinky_ai_sessions s ON s.subject_key=b.subject_key
                     JOIN pinky_ai_entitlements e ON e.subject_key=b.subject_key
                     JOIN accounts a ON a.id=b.account_id
                     WHERE b.subject_key=?1 AND b.revoked_at IS NULL
                       AND s.id=?2 AND s.state='active' AND s.expires_at>?3
                       AND e.kind='synthetic_preprod' AND e.state='active'
                       AND e.expires_at>?3 AND e.credit_cents>0
                       AND (a.balance_cents>0 OR EXISTS(
                           SELECT 1 FROM pinky_ai_requests r
                           WHERE r.request_id=?5 AND r.subject_key=?1 AND r.session_id=?2))
                       AND a.billing_restricted=0
                       AND a.password_hash=?4 AND a.is_admin=0
                       AND a.trial_seconds_remaining=0 AND a.auto_topup_enabled=0
                       AND a.is_temporary=0 AND a.temporary_expires_at IS NULL
                       AND a.stripe_customer_id IS NULL
                       AND a.stripe_payment_method_id IS NULL
                       AND a.square_customer_id IS NULL AND a.square_card_id IS NULL",
                    params![key, session_id, now, EXTERNAL_PASSWORD_SENTINEL, request_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or(StoreError::Denied)?;
            let existing = tx
                .query_row(
                    "SELECT subject_key,session_id,prompt_sha256,response_mode,state,
                            accounting_status
                     FROM pinky_ai_requests WHERE request_id=?1",
                    [request_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    },
                )
                .optional()?;
            let status = if let Some((owner, session, digest, mode, state, accounting)) = existing {
                if owner != key
                    || session != session_id
                    || digest != prompt_sha256
                    || mode != response_mode
                {
                    return Err(StoreError::Denied.into());
                }
                request_status(session_id, request_id, state, accounting)
            } else {
                tx.execute(
                    "INSERT INTO pinky_ai_requests
                     (request_id,subject_key,session_id,prompt_sha256,response_mode,state,
                      accounting_status,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,?5,'admitted','pending',?6,?6)",
                    params![
                        request_id,
                        key,
                        session_id,
                        prompt_sha256,
                        response_mode,
                        now
                    ],
                )?;
                request_status(session_id, request_id, "admitted".into(), "pending".into())
            };
            tx.commit()?;
            Ok((account_id, status))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one("SELECT pg_advisory_xact_lock(1112299865,627)", &[])?;
            if let Some(row) = tx.query_opt(
                "SELECT subject_key,session_id FROM pinky_ai_cancel_tombstones
                 WHERE request_id=$1",
                &[&request_id],
            )? {
                return Err(
                    if row.get::<_, String>(0) == key && row.get::<_, String>(1) == session_id {
                        StoreError::Cancelled.into()
                    } else {
                        StoreError::Denied.into()
                    },
                );
            }
            let account_id = tx
                .query_opt(
                    "SELECT b.account_id
                     FROM pinky_ai_bindings b
                     JOIN pinky_ai_sessions s ON s.subject_key=b.subject_key
                     JOIN pinky_ai_entitlements e ON e.subject_key=b.subject_key
                     JOIN accounts a ON a.id=b.account_id
                     WHERE b.subject_key=$1 AND b.revoked_at IS NULL
                       AND s.id=$2 AND s.state='active' AND s.expires_at>$3
                       AND e.kind='synthetic_preprod' AND e.state='active'
                       AND e.expires_at>$3 AND e.credit_cents>0
                       AND (a.balance_cents>0 OR EXISTS(
                           SELECT 1 FROM pinky_ai_requests r
                           WHERE r.request_id=$5 AND r.subject_key=$1 AND r.session_id=$2))
                       AND a.billing_restricted=0
                       AND a.password_hash=$4 AND a.is_admin=0
                       AND a.trial_seconds_remaining=0 AND a.auto_topup_enabled=0
                       AND a.is_temporary=0 AND a.temporary_expires_at IS NULL
                       AND a.stripe_customer_id IS NULL
                       AND a.stripe_payment_method_id IS NULL
                       AND a.square_customer_id IS NULL AND a.square_card_id IS NULL
                     FOR UPDATE OF b,s,e,a",
                    &[
                        &key,
                        &session_id,
                        &now,
                        &EXTERNAL_PASSWORD_SENTINEL,
                        &request_id,
                    ],
                )?
                .map(|row| row.get::<_, String>(0))
                .ok_or(StoreError::Denied)?;
            let status = if let Some(row) = tx.query_opt(
                "SELECT subject_key,session_id,prompt_sha256,response_mode,state,
                        accounting_status
                 FROM pinky_ai_requests WHERE request_id=$1 FOR UPDATE",
                &[&request_id],
            )? {
                if row.get::<_, String>(0) != key
                    || row.get::<_, String>(1) != session_id
                    || row.get::<_, String>(2) != prompt_sha256
                    || row.get::<_, String>(3) != response_mode
                {
                    return Err(StoreError::Denied.into());
                }
                request_status(session_id, request_id, row.get(4), row.get(5))
            } else {
                tx.execute(
                    "INSERT INTO pinky_ai_requests
                     (request_id,subject_key,session_id,prompt_sha256,response_mode,state,
                      accounting_status,created_at,updated_at)
                     VALUES ($1,$2,$3,$4,$5,'admitted','pending',$6,$6)",
                    &[
                        &request_id,
                        &key,
                        &session_id,
                        &prompt_sha256,
                        &response_mode,
                        &now,
                    ],
                )?;
                request_status(session_id, request_id, "admitted".into(), "pending".into())
            };
            tx.commit()?;
            Ok((account_id, status))
        }
    })
}

pub fn mark_request_running(
    pool: &DbPool,
    key: &str,
    session_id: &str,
    request_id: &str,
    now: i64,
) -> Result<bool> {
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "UPDATE pinky_ai_requests SET state='running',updated_at=?4
             WHERE subject_key=?1 AND session_id=?2 AND request_id=?3 AND state='admitted'",
            params![key, session_id, request_id, now],
        )? == 1),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "UPDATE pinky_ai_requests SET state='running',updated_at=$4
             WHERE subject_key=$1 AND session_id=$2 AND request_id=$3 AND state='admitted'",
            &[&key, &session_id, &request_id, &now],
        )? == 1),
    })
}

pub fn dispatch_allowed(
    pool: &DbPool,
    key: &str,
    session_id: &str,
    request_id: &str,
    now: i64,
) -> bool {
    let result: Result<bool> = run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM pinky_ai_requests r
               JOIN pinky_ai_sessions s ON s.id=r.session_id AND s.subject_key=r.subject_key
               JOIN pinky_ai_bindings b ON b.subject_key=r.subject_key
               JOIN pinky_ai_entitlements e ON e.subject_key=r.subject_key
               JOIN accounts a ON a.id=b.account_id
                 WHERE r.subject_key=?1 AND r.session_id=?2 AND r.request_id=?3
                 AND r.state IN ('admitted','running','finished')
                 AND s.state='active' AND s.expires_at>?4 AND b.revoked_at IS NULL
                 AND e.kind='synthetic_preprod' AND e.state='active'
                 AND e.expires_at>?4 AND e.credit_cents>0
                 AND (a.balance_cents>0 OR EXISTS(
                     SELECT 1 FROM usage_reservations u
                     WHERE u.account_id=a.id AND u.request_id=r.request_id
                       AND u.status IN ('reserved','settled')))
                 AND a.billing_restricted=0 AND a.password_hash=?5
                 AND a.is_admin=0 AND a.trial_seconds_remaining=0
                 AND a.auto_topup_enabled=0 AND a.is_temporary=0
                 AND a.temporary_expires_at IS NULL
                 AND a.stripe_customer_id IS NULL AND a.stripe_payment_method_id IS NULL
                 AND a.square_customer_id IS NULL AND a.square_card_id IS NULL)",
            params![key, session_id, request_id, now, EXTERNAL_PASSWORD_SENTINEL],
            |row| row.get::<_, bool>(0),
        )?),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_one(
                "SELECT EXISTS(
               SELECT 1 FROM pinky_ai_requests r
               JOIN pinky_ai_sessions s ON s.id=r.session_id AND s.subject_key=r.subject_key
               JOIN pinky_ai_bindings b ON b.subject_key=r.subject_key
               JOIN pinky_ai_entitlements e ON e.subject_key=r.subject_key
               JOIN accounts a ON a.id=b.account_id
                 WHERE r.subject_key=$1 AND r.session_id=$2 AND r.request_id=$3
                 AND r.state IN ('admitted','running','finished')
                 AND s.state='active' AND s.expires_at>$4 AND b.revoked_at IS NULL
                 AND e.kind='synthetic_preprod' AND e.state='active'
                 AND e.expires_at>$4 AND e.credit_cents>0
                 AND (a.balance_cents>0 OR EXISTS(
                     SELECT 1 FROM usage_reservations u
                     WHERE u.account_id=a.id AND u.request_id=r.request_id
                       AND u.status IN ('reserved','settled')))
                 AND a.billing_restricted=0 AND a.password_hash=$5
                 AND a.is_admin=0 AND a.trial_seconds_remaining=0
                 AND a.auto_topup_enabled=0 AND a.is_temporary=0
                 AND a.temporary_expires_at IS NULL
                 AND a.stripe_customer_id IS NULL AND a.stripe_payment_method_id IS NULL
                 AND a.square_customer_id IS NULL AND a.square_card_id IS NULL)",
                &[
                    &key,
                    &session_id,
                    &request_id,
                    &now,
                    &EXTERNAL_PASSWORD_SENTINEL,
                ],
            )?
            .get::<_, bool>(0)),
    });
    result.unwrap_or(false)
}

pub fn cancel_request(
    pool: &DbPool,
    key: &str,
    session_id: &str,
    request_id: &str,
    now: i64,
) -> Result<Option<AiRequestStatus>> {
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "UPDATE pinky_ai_requests
                 SET state='cancellation_requested',updated_at=?4
                 WHERE subject_key=?1 AND session_id=?2 AND request_id=?3
                   AND state IN ('admitted','running')",
                params![key, session_id, request_id, now],
            )?;
            let existing = tx
                .query_row(
                    "SELECT subject_key,session_id,state,accounting_status
                     FROM pinky_ai_requests WHERE request_id=?1",
                    [request_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((owner, session, state, accounting)) = existing {
                let result = (owner == key && session == session_id)
                    .then(|| request_status(session_id, request_id, state, accounting));
                tx.commit()?;
                return Ok(result);
            }
            let tombstone = tx
                .query_row(
                    "SELECT subject_key,session_id FROM pinky_ai_cancel_tombstones
                     WHERE request_id=?1",
                    [request_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            if let Some((owner, session)) = tombstone {
                let result = (owner == key && session == session_id).then(|| {
                    request_status(session_id, request_id, "cancelled".into(), "settled".into())
                });
                tx.commit()?;
                return Ok(result);
            }
            let session_owned: bool = tx.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM pinky_ai_sessions s
                   JOIN pinky_ai_bindings b ON b.subject_key=s.subject_key
                   WHERE s.id=?1 AND s.subject_key=?2)",
                params![session_id, key],
                |row| row.get(0),
            )?;
            if !session_owned {
                tx.commit()?;
                return Ok(None);
            }
            let counts: (i64, i64) = tx.query_row(
                "SELECT (SELECT COUNT(*) FROM pinky_ai_cancel_tombstones),
                        (SELECT COUNT(*) FROM pinky_ai_cancel_tombstones
                         WHERE subject_key=?1 AND session_id=?2)",
                params![key, session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if counts.0 >= MAX_CANCEL_TOMBSTONES || counts.1 >= MAX_CANCEL_TOMBSTONES_PER_SESSION {
                return Err(StoreError::Capacity.into());
            }
            tx.execute(
                "INSERT INTO pinky_ai_cancel_tombstones
                 (request_id,subject_key,session_id,created_at) VALUES (?1,?2,?3,?4)",
                params![request_id, key, session_id, now],
            )?;
            tx.commit()?;
            Ok(Some(request_status(
                session_id,
                request_id,
                "cancelled".into(),
                "settled".into(),
            )))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one("SELECT pg_advisory_xact_lock(1112299865,627)", &[])?;
            tx.execute(
                "UPDATE pinky_ai_requests
                 SET state='cancellation_requested',updated_at=$4
                 WHERE subject_key=$1 AND session_id=$2 AND request_id=$3
                   AND state IN ('admitted','running')",
                &[&key, &session_id, &request_id, &now],
            )?;
            if let Some(row) = tx.query_opt(
                "SELECT subject_key,session_id,state,accounting_status
                 FROM pinky_ai_requests WHERE request_id=$1",
                &[&request_id],
            )? {
                let result = (row.get::<_, String>(0) == key
                    && row.get::<_, String>(1) == session_id)
                    .then(|| request_status(session_id, request_id, row.get(2), row.get(3)));
                tx.commit()?;
                return Ok(result);
            }
            if let Some(row) = tx.query_opt(
                "SELECT subject_key,session_id FROM pinky_ai_cancel_tombstones
                 WHERE request_id=$1",
                &[&request_id],
            )? {
                let result = (row.get::<_, String>(0) == key
                    && row.get::<_, String>(1) == session_id)
                    .then(|| {
                        request_status(session_id, request_id, "cancelled".into(), "settled".into())
                    });
                tx.commit()?;
                return Ok(result);
            }
            let session_owned: bool = tx
                .query_one(
                    "SELECT EXISTS(
                       SELECT 1 FROM pinky_ai_sessions s
                       JOIN pinky_ai_bindings b ON b.subject_key=s.subject_key
                       WHERE s.id=$1 AND s.subject_key=$2)",
                    &[&session_id, &key],
                )?
                .get(0);
            if !session_owned {
                tx.commit()?;
                return Ok(None);
            }
            let counts = tx.query_one(
                "SELECT (SELECT COUNT(*) FROM pinky_ai_cancel_tombstones),
                        (SELECT COUNT(*) FROM pinky_ai_cancel_tombstones
                         WHERE subject_key=$1 AND session_id=$2)",
                &[&key, &session_id],
            )?;
            if counts.get::<_, i64>(0) >= MAX_CANCEL_TOMBSTONES
                || counts.get::<_, i64>(1) >= MAX_CANCEL_TOMBSTONES_PER_SESSION
            {
                return Err(StoreError::Capacity.into());
            }
            tx.execute(
                "INSERT INTO pinky_ai_cancel_tombstones
                 (request_id,subject_key,session_id,created_at) VALUES ($1,$2,$3,$4)",
                &[&request_id, &key, &session_id, &now],
            )?;
            tx.commit()?;
            Ok(Some(request_status(
                session_id,
                request_id,
                "cancelled".into(),
                "settled".into(),
            )))
        }
    })
}

pub fn status(
    pool: &DbPool,
    key: &str,
    session_id: &str,
    request_id: &str,
) -> Result<Option<AiRequestStatus>> {
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool
            .get()?
            .query_row(
                "SELECT state,accounting_status FROM pinky_ai_requests
                 WHERE subject_key=?1 AND session_id=?2 AND request_id=?3
                 UNION ALL
                 SELECT 'cancelled','settled' FROM pinky_ai_cancel_tombstones
                 WHERE subject_key=?1 AND session_id=?2 AND request_id=?3
                 LIMIT 1",
                params![key, session_id, request_id],
                |row| {
                    Ok(request_status(
                        session_id,
                        request_id,
                        row.get(0)?,
                        row.get(1)?,
                    ))
                },
            )
            .optional()?),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_opt(
                "SELECT state,accounting_status FROM pinky_ai_requests
                 WHERE subject_key=$1 AND session_id=$2 AND request_id=$3
                 UNION ALL
                 SELECT 'cancelled','settled' FROM pinky_ai_cancel_tombstones
                 WHERE subject_key=$1 AND session_id=$2 AND request_id=$3
                 LIMIT 1",
                &[&key, &session_id, &request_id],
            )?
            .map(|row| request_status(session_id, request_id, row.get(0), row.get(1)))),
    })
}

pub fn finish_request(
    pool: &DbPool,
    key: &str,
    session_id: &str,
    request_id: &str,
    failed: bool,
    accounting_settled: bool,
    now: i64,
) -> Result<Option<AiRequestStatus>> {
    run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "UPDATE pinky_ai_requests
                 SET state=CASE WHEN state='cancellation_requested' AND ?5 THEN 'cancelled'
                                WHEN state='cancellation_requested' THEN 'cancellation_requested'
                                WHEN ?4 THEN 'failed' ELSE 'finished' END,
                     accounting_status=CASE WHEN ?5 THEN 'settled' ELSE 'pending' END,
                     updated_at=?6,
                     terminal_at=CASE WHEN ?5 THEN COALESCE(terminal_at,?6) ELSE terminal_at END
                 WHERE subject_key=?1 AND session_id=?2 AND request_id=?3
                   AND state IN ('admitted','running','cancellation_requested')",
                params![key, session_id, request_id, failed, accounting_settled, now],
            )?;
            drop(conn);
            status(pool, key, session_id, request_id)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "UPDATE pinky_ai_requests
                 SET state=CASE WHEN state='cancellation_requested' AND $5 THEN 'cancelled'
                                WHEN state='cancellation_requested' THEN 'cancellation_requested'
                                WHEN $4 THEN 'failed' ELSE 'finished' END,
                     accounting_status=CASE WHEN $5 THEN 'settled' ELSE 'pending' END,
                     updated_at=$6,
                     terminal_at=CASE WHEN $5 THEN COALESCE(terminal_at,$6) ELSE terminal_at END
                 WHERE subject_key=$1 AND session_id=$2 AND request_id=$3
                   AND state IN ('admitted','running','cancellation_requested')",
                &[
                    &key,
                    &session_id,
                    &request_id,
                    &failed,
                    &accounting_settled,
                    &now,
                ],
            )?;
            drop(conn);
            status(pool, key, session_id, request_id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> (DbPool, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("pinky-ai-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        initialize(&pool).unwrap();
        (pool, path)
    }

    #[test]
    fn owned_sessions_are_idempotent_and_do_not_reopen() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "subject-a");
        let other = subject_key("pinky", "bluey", "preprod", "subject-b");
        let id = uuid::Uuid::new_v4().to_string();
        let created = open(&pool, &key, &id, 1000).unwrap();
        assert_eq!(created.access, "not_added");
        assert_eq!(created, open(&pool, &key, &id, 1100).unwrap());
        assert!(open(&pool, &other, &id, 1100).is_err());
        assert!(close(&pool, &other, &id, 1100).unwrap().is_none());
        close(&pool, &key, &id, 1200).unwrap().unwrap();
        let closed = open(&pool, &key, &id, 1300).unwrap();
        assert_eq!(closed.state, "closed");
        assert_eq!(closed.access, "unavailable");
        let account = crate::db::accounts::Account::fetch_by_id(&pool, &format!("pinky_{key}"))
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 0);
        assert_eq!(account.trial_seconds_remaining, 0);
        assert!(!account.is_admin && !account.auto_topup_enabled);
        assert!(
            crate::db::accounts::Account::fetch_by_id(&pool, &format!("pinky_{other}"))
                .unwrap()
                .is_none()
        );
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn environment_and_revocation_are_not_interchangeable() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "subject-a");
        assert_ne!(
            key,
            subject_key("pinky", "bluey", "production", "subject-a")
        );
        assert_ne!(key, subject_key("pinky", "other", "preprod", "subject-a"));
        let id = uuid::Uuid::new_v4().to_string();
        open(&pool, &key, &id, 1000).unwrap();
        let expired = open(&pool, &key, &id, 4600).unwrap();
        assert_eq!(expired.state, "expired");
        assert_eq!(expired.access, "unavailable");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE pinky_ai_bindings SET revoked_at=2000 WHERE subject_key=?1",
                [&key],
            )
            .unwrap();
        assert_eq!(
            sqlite_access(&pool.get().unwrap(), &key, &id, 2100).unwrap(),
            "unavailable"
        );
        assert!(open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 2100).is_err());
        assert!(close(&pool, &key, &id, 2100).unwrap().is_some());
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ask_requires_separate_entitlement_and_exact_active_context() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "entitled");
        let session_id = uuid::Uuid::new_v4().to_string();
        open(&pool, &key, &session_id, 1000).unwrap();
        let request_id = uuid::Uuid::new_v4().to_string();
        assert_domain(
            admit_ask(
                &pool,
                &key,
                &session_id,
                &request_id,
                &"a".repeat(64),
                "default",
                1001,
            )
            .unwrap_err(),
            false,
        );

        let entitled_key = subject_key("pinky", "bluey", "preprod", "entitled-2");
        let entitled_session = uuid::Uuid::new_v4().to_string();
        let entitled = open_with_entitlement(
            &pool,
            &entitled_key,
            &entitled_session,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        assert_eq!(entitled.state, "active");
        assert_eq!(entitled.access, "available");
        let request_id = uuid::Uuid::new_v4().to_string();
        let (_, admitted) = admit_ask(
            &pool,
            &entitled_key,
            &entitled_session,
            &request_id,
            &"b".repeat(64),
            "short",
            1001,
        )
        .unwrap();
        assert_eq!(admitted.state, "admitted");
        assert!(
            mark_request_running(&pool, &entitled_key, &entitled_session, &request_id, 1002)
                .unwrap()
        );
        assert!(dispatch_allowed(
            &pool,
            &entitled_key,
            &entitled_session,
            &request_id,
            1003
        ));
        close(&pool, &entitled_key, &entitled_session, 1004)
            .unwrap()
            .unwrap();
        assert!(!dispatch_allowed(
            &pool,
            &entitled_key,
            &entitled_session,
            &request_id,
            1005
        ));
        let status = status(&pool, &entitled_key, &entitled_session, &request_id)
            .unwrap()
            .unwrap();
        assert_eq!(status.state, "cancellation_requested");
        assert_eq!(status.accounting_status, "pending");
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ask_request_identity_and_cancel_are_exact_and_idempotent() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "owner");
        let other = subject_key("pinky", "bluey", "preprod", "other");
        let session_id = uuid::Uuid::new_v4().to_string();
        open_with_entitlement(
            &pool,
            &key,
            &session_id,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        let request_id = uuid::Uuid::new_v4().to_string();
        let digest = "c".repeat(64);
        admit_ask(&pool, &key, &session_id, &request_id, &digest, "star", 1001).unwrap();
        assert!(admit_ask(
            &pool,
            &key,
            &session_id,
            &request_id,
            &"d".repeat(64),
            "star",
            1002,
        )
        .is_err());
        assert!(
            cancel_request(&pool, &other, &session_id, &request_id, 1002)
                .unwrap()
                .is_none()
        );
        let first = cancel_request(&pool, &key, &session_id, &request_id, 1003)
            .unwrap()
            .unwrap();
        let replay = cancel_request(&pool, &key, &session_id, &request_id, 1004)
            .unwrap()
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(first.state, "cancellation_requested");
        assert_eq!(first.accounting_status, "pending");
        let terminal = finish_request(&pool, &key, &session_id, &request_id, false, true, 1005)
            .unwrap()
            .unwrap();
        assert_eq!(terminal.state, "cancelled");
        assert_eq!(terminal.accounting_status, "settled");
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn exhausted_provider_release_is_terminal_failed_not_pending() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "exhausted-provider");
        let session_id = uuid::Uuid::new_v4().to_string();
        open_with_entitlement(
            &pool,
            &key,
            &session_id,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        let request_id = uuid::Uuid::new_v4().to_string();
        admit_ask(
            &pool,
            &key,
            &session_id,
            &request_id,
            &"e".repeat(64),
            "default",
            1001,
        )
        .unwrap();
        assert!(mark_request_running(&pool, &key, &session_id, &request_id, 1002).unwrap());

        let terminal = finish_request(&pool, &key, &session_id, &request_id, true, true, 1003)
            .unwrap()
            .unwrap();
        assert_eq!(terminal.state, "failed");
        assert_eq!(terminal.accounting_status, "settled");
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn concurrent_exact_replays_have_one_running_claimant() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "concurrent-replay");
        let session_id = uuid::Uuid::new_v4().to_string();
        open_with_entitlement(
            &pool,
            &key,
            &session_id,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        let request_id = uuid::Uuid::new_v4().to_string();
        admit_ask(
            &pool,
            &key,
            &session_id,
            &request_id,
            &"f".repeat(64),
            "default",
            1001,
        )
        .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut claims = Vec::new();
        for _ in 0..2 {
            let thread_pool = pool.clone();
            let thread_key = key.clone();
            let thread_session = session_id.clone();
            let thread_request = request_id.clone();
            let thread_barrier = barrier.clone();
            claims.push(std::thread::spawn(move || {
                thread_barrier.wait();
                mark_request_running(
                    &thread_pool,
                    &thread_key,
                    &thread_session,
                    &thread_request,
                    1002,
                )
                .unwrap()
            }));
        }
        let wins = claims
            .into_iter()
            .map(|claim| claim.join().unwrap())
            .filter(|claimed| *claimed)
            .count();
        assert_eq!(wins, 1);
        let stable = status(&pool, &key, &session_id, &request_id)
            .unwrap()
            .unwrap();
        assert_eq!(stable.state, "running");
        assert_eq!(stable.accounting_status, "pending");
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stop_before_ask_tombstones_exact_owned_request() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "stop-before-ask");
        let other = subject_key("pinky", "bluey", "preprod", "other-stop");
        let session_id = uuid::Uuid::new_v4().to_string();
        let other_session = uuid::Uuid::new_v4().to_string();
        open_with_entitlement(
            &pool,
            &key,
            &session_id,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        open_with_entitlement(
            &pool,
            &other,
            &other_session,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        let request_id = uuid::Uuid::new_v4().to_string();
        let first = cancel_request(&pool, &key, &session_id, &request_id, 1001)
            .unwrap()
            .unwrap();
        let replay = cancel_request(&pool, &key, &session_id, &request_id, 1002)
            .unwrap()
            .unwrap();
        assert_eq!(first, replay);
        assert_eq!(first.state, "cancelled");
        assert_eq!(first.accounting_status, "settled");
        assert!(
            cancel_request(&pool, &other, &other_session, &request_id, 1002)
                .unwrap()
                .is_none()
        );
        let error = admit_ask(
            &pool,
            &key,
            &session_id,
            &request_id,
            &"a".repeat(64),
            "default",
            1003,
        )
        .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<StoreError>(),
            Some(StoreError::Cancelled)
        ));
        assert!(!dispatch_allowed(
            &pool,
            &key,
            &session_id,
            &request_id,
            1004
        ));
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn concurrent_stop_and_admission_always_leave_dispatch_fenced() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "stop-admit-race");
        let session_id = uuid::Uuid::new_v4().to_string();
        open_with_entitlement(
            &pool,
            &key,
            &session_id,
            1000,
            Some(SyntheticEntitlement {
                credit_cents: 1500,
                ttl_seconds: 3600,
            }),
        )
        .unwrap();
        let request_id = uuid::Uuid::new_v4().to_string();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let cancel_pool = pool.clone();
        let cancel_key = key.clone();
        let cancel_session = session_id.clone();
        let cancel_request_id = request_id.clone();
        let cancel_barrier = barrier.clone();
        let cancel = std::thread::spawn(move || {
            cancel_barrier.wait();
            cancel_request(
                &cancel_pool,
                &cancel_key,
                &cancel_session,
                &cancel_request_id,
                1001,
            )
            .unwrap()
            .unwrap()
        });
        let admit_pool = pool.clone();
        let admit_key = key.clone();
        let admit_session = session_id.clone();
        let admit_request_id = request_id.clone();
        let admission = std::thread::spawn(move || {
            barrier.wait();
            admit_ask(
                &admit_pool,
                &admit_key,
                &admit_session,
                &admit_request_id,
                &"b".repeat(64),
                "default",
                1001,
            )
        });
        let cancelled = cancel.join().unwrap();
        let admitted = admission.join().unwrap();
        assert!(matches!(
            cancelled.state.as_str(),
            "cancelled" | "cancellation_requested"
        ));
        if let Err(error) = admitted {
            assert!(matches!(
                error.downcast_ref::<StoreError>(),
                Some(StoreError::Cancelled)
            ));
        }
        let final_status = status(&pool, &key, &session_id, &request_id)
            .unwrap()
            .unwrap();
        assert!(matches!(
            final_status.state.as_str(),
            "cancelled" | "cancellation_requested"
        ));
        assert!(!dispatch_allowed(
            &pool,
            &key,
            &session_id,
            &request_id,
            1002
        ));
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    fn postgres_fixture(pool: &DbPool, label: &str) -> (String, String, String) {
        let subject = format!("{label}-{}", uuid::Uuid::new_v4());
        let key = subject_key("pinky", "bluey", "preprod", &subject);
        let session_id = uuid::Uuid::new_v4().to_string();
        let opened = open_with_entitlement(
            pool,
            &key,
            &session_id,
            1_000,
            Some(SyntheticEntitlement {
                credit_cents: 50,
                ttl_seconds: 3_600,
            }),
        )
        .expect("open PostgreSQL Pinky fixture");
        assert_eq!(opened.state, "active");
        assert_eq!(opened.access, "available");
        (format!("pinky_{key}"), key, session_id)
    }

    #[test]
    #[serial_test::serial]
    fn postgres_lifecycle_claim_and_cancel_admit_use_real_store_transactions() {
        if std::env::var("BLUEY_PINKY_TEST_POSTGRES_EPHEMERAL")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }
        let database_url = std::env::var("BLUEY_TEST_POSTGRES_URL")
            .expect("ephemeral PostgreSQL test URL is required");
        let pool = crate::db::open_postgres_pool(&database_url)
            .expect("open ephemeral PostgreSQL test pool");
        initialize(&pool).expect("initialize PostgreSQL Pinky schema");
        let mut account_ids = Vec::new();

        let (account_id, key, session_id) = postgres_fixture(&pool, "lifecycle");
        account_ids.push(account_id);
        let replay = open_with_entitlement(&pool, &key, &session_id, 1_100, None)
            .expect("replay exact PostgreSQL session");
        assert_eq!(replay.state, "active");
        assert_eq!(replay.access, "available");
        let closed = close(&pool, &key, &session_id, 1_200)
            .expect("close PostgreSQL session")
            .expect("owned PostgreSQL session");
        assert_eq!(closed.state, "closed");
        assert_eq!(closed.access, "unavailable");

        let (account_id, key, session_id) = postgres_fixture(&pool, "single-claim");
        account_ids.push(account_id);
        let request_id = uuid::Uuid::new_v4().to_string();
        admit_ask(
            &pool,
            &key,
            &session_id,
            &request_id,
            &"f".repeat(64),
            "default",
            1_001,
        )
        .expect("admit PostgreSQL claim fixture");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let claims = (0..2)
            .map(|_| {
                let pool = pool.clone();
                let key = key.clone();
                let session_id = session_id.clone();
                let request_id = request_id.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    mark_request_running(&pool, &key, &session_id, &request_id, 1_002)
                        .expect("claim PostgreSQL request")
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            claims
                .into_iter()
                .map(|claim| claim.join().expect("join PostgreSQL claim"))
                .filter(|claimed| *claimed)
                .count(),
            1
        );
        let running = status(&pool, &key, &session_id, &request_id)
            .expect("read PostgreSQL claim status")
            .expect("PostgreSQL claim status");
        assert_eq!(running.state, "running");
        assert_eq!(running.accounting_status, "pending");

        let (account_id, key, session_id) = postgres_fixture(&pool, "stop-before-ask");
        account_ids.push(account_id);
        let request_id = uuid::Uuid::new_v4().to_string();
        let first = cancel_request(&pool, &key, &session_id, &request_id, 1_001)
            .expect("create PostgreSQL cancel tombstone")
            .expect("owned PostgreSQL session");
        let replay = cancel_request(&pool, &key, &session_id, &request_id, 1_002)
            .expect("replay PostgreSQL cancel tombstone")
            .expect("owned PostgreSQL tombstone");
        assert_eq!(first, replay);
        assert_eq!(first.state, "cancelled");
        assert_eq!(first.accounting_status, "settled");
        let error = admit_ask(
            &pool,
            &key,
            &session_id,
            &request_id,
            &"a".repeat(64),
            "short",
            1_003,
        )
        .expect_err("a PostgreSQL tombstone must reject late admission");
        assert!(matches!(
            error.downcast_ref::<StoreError>(),
            Some(StoreError::Cancelled)
        ));
        assert!(!dispatch_allowed(
            &pool,
            &key,
            &session_id,
            &request_id,
            1_004
        ));

        let (account_id, key, session_id) = postgres_fixture(&pool, "stop-admit-race");
        account_ids.push(account_id);
        for round in 0_i64..12 {
            let request_id = uuid::Uuid::new_v4().to_string();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let cancel_pool = pool.clone();
            let cancel_key = key.clone();
            let cancel_session = session_id.clone();
            let cancel_request_id = request_id.clone();
            let cancel_barrier = barrier.clone();
            let cancel = std::thread::spawn(move || {
                cancel_barrier.wait();
                cancel_request(
                    &cancel_pool,
                    &cancel_key,
                    &cancel_session,
                    &cancel_request_id,
                    1_010 + round,
                )
                .expect("race PostgreSQL cancellation")
                .expect("owned PostgreSQL race session")
            });
            let admit_pool = pool.clone();
            let admit_key = key.clone();
            let admit_session = session_id.clone();
            let admit_request = request_id.clone();
            let admission = std::thread::spawn(move || {
                barrier.wait();
                admit_ask(
                    &admit_pool,
                    &admit_key,
                    &admit_session,
                    &admit_request,
                    &"b".repeat(64),
                    "star",
                    1_010 + round,
                )
            });
            let cancelled = cancel.join().expect("join PostgreSQL cancellation");
            let admitted = admission.join().expect("join PostgreSQL admission");
            assert!(matches!(
                cancelled.state.as_str(),
                "cancelled" | "cancellation_requested"
            ));
            if let Err(error) = admitted {
                assert!(matches!(
                    error.downcast_ref::<StoreError>(),
                    Some(StoreError::Cancelled)
                ));
            }
            let final_status = status(&pool, &key, &session_id, &request_id)
                .expect("read PostgreSQL race status")
                .expect("PostgreSQL race status");
            assert!(matches!(
                final_status.state.as_str(),
                "cancelled" | "cancellation_requested"
            ));
            assert!(!dispatch_allowed(
                &pool,
                &key,
                &session_id,
                &request_id,
                1_100 + round
            ));
        }

        let mut conn = pool.get_pg().expect("get PostgreSQL cleanup connection");
        for account_id in account_ids {
            conn.execute("DELETE FROM accounts WHERE id=$1", &[&account_id])
                .expect("delete PostgreSQL Pinky fixture");
        }
    }

    fn assert_domain(error: anyhow::Error, capacity: bool) {
        assert!(matches!(
            (error.downcast_ref::<StoreError>(), capacity),
            (Some(StoreError::Capacity), true) | (Some(StoreError::Denied), false)
        ));
    }

    #[test]
    fn unrelated_deterministic_accounts_never_become_bindings() {
        let (pool, path) = pool();
        for privileged in [0, 1] {
            let key = subject_key(
                "pinky",
                "bluey",
                "preprod",
                &format!("collision-{privileged}"),
            );
            let account_id = format!("pinky_{key}");
            pool.get().unwrap().execute(
                "INSERT INTO accounts(id,email,password_hash,balance_cents,is_admin,
                 trial_seconds_remaining,auto_topup_enabled) VALUES (?1,?2,'!unrelated',500,?3,900,1)",
                params![account_id, format!("unrelated-{privileged}@example.invalid"), privileged],
            ).unwrap();
            assert_domain(
                open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 1000).unwrap_err(),
                false,
            );
            let count: i64 = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM pinky_ai_bindings WHERE subject_key=?1",
                    [&key],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
            let account = crate::db::accounts::Account::fetch_by_id(&pool, &account_id)
                .unwrap()
                .unwrap();
            assert_eq!(account.balance_cents, 500);
        }
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn existing_origin_preserves_credits_but_denies_escalated_account_state() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "a");
        let id = uuid::Uuid::new_v4().to_string();
        open(&pool, &key, &id, 1000).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE accounts SET balance_cents=1500 WHERE id=?1",
                [format!("pinky_{key}")],
            )
            .unwrap();
        assert_eq!(open(&pool, &key, &id, 1001).unwrap().state, "active");
        let statements = [
            "UPDATE accounts SET auto_topup_enabled=1",
            "UPDATE accounts SET is_admin=1",
            "UPDATE accounts SET trial_seconds_remaining=1",
            "UPDATE accounts SET is_temporary=1",
            "UPDATE accounts SET billing_restricted=1",
            "UPDATE accounts SET password_hash='!changed'",
            "UPDATE accounts SET stripe_customer_id='synthetic'",
        ];
        for sql in statements {
            let conn = pool.get().unwrap();
            conn.execute(sql, []).unwrap();
            assert_domain(open(&pool, &key, &id, 1002).unwrap_err(), false);
            conn.execute(
                "UPDATE accounts SET auto_topup_enabled=0,is_admin=0,trial_seconds_remaining=0,
                 is_temporary=0,billing_restricted=0,password_hash=?1,stripe_customer_id=NULL",
                [EXTERNAL_PASSWORD_SENTINEL],
            )
            .unwrap();
        }
        assert_eq!(
            crate::db::accounts::Account::fetch_by_id(&pool, &format!("pinky_{key}"))
                .unwrap()
                .unwrap()
                .balance_cents,
            1500
        );
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn active_quota_counts_only_live_sessions_and_replays_remain_idempotent() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "a");
        let ids: Vec<_> = (0..4).map(|_| uuid::Uuid::new_v4().to_string()).collect();
        for id in &ids[..3] {
            open(&pool, &key, id, 1000).unwrap();
        }
        assert_domain(open(&pool, &key, &ids[3], 1000).unwrap_err(), true);
        assert!(open(&pool, &key, &ids[0], 1000).is_ok());
        close(&pool, &key, &ids[0], 1000).unwrap();
        open(&pool, &key, &ids[3], 1000).unwrap();
        assert_eq!(open(&pool, &key, &ids[0], 1000).unwrap().state, "closed");
        open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 4600).unwrap();
        assert_eq!(open(&pool, &key, &ids[1], 4600).unwrap().state, "expired");
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn durable_total_and_per_subject_limits_do_not_delete_replay_tombstones() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "a");
        let id = uuid::Uuid::new_v4().to_string();
        open(&pool, &key, &id, 1000).unwrap();
        close(&pool, &key, &id, 1000).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<999)
             INSERT INTO pinky_ai_sessions(id,subject_key,state,created_at,expires_at,closed_at)
             SELECT 'fixture-'||x,?1,'closed',1,2,2 FROM n",
                [&key],
            )
            .unwrap();
        assert_domain(
            open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 1000).unwrap_err(),
            true,
        );
        assert_eq!(open(&pool, &key, &id, 1000).unwrap().state, "closed");
        pool.get()
            .unwrap()
            .execute(
                "WITH RECURSIVE n(x) AS (SELECT 1000 UNION ALL SELECT x+1 FROM n WHERE x<9999)
             INSERT INTO pinky_ai_sessions(id,subject_key,state,created_at,expires_at,closed_at)
             SELECT 'fixture-'||x,?1,'closed',1,2,2 FROM n",
                [&key],
            )
            .unwrap();
        let other = subject_key("pinky", "bluey", "preprod", "b");
        assert_domain(
            open(&pool, &other, &uuid::Uuid::new_v4().to_string(), 1000).unwrap_err(),
            true,
        );
        assert!(
            crate::db::accounts::Account::fetch_by_id(&pool, &format!("pinky_{other}"))
                .unwrap()
                .is_none()
        );
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn binding_limit_is_durable_and_denied_enrollment_is_atomic() {
        let (pool, path) = pool();
        for i in 0..MAX_BINDINGS {
            let key = subject_key("pinky", "bluey", "preprod", &format!("subject-{i}"));
            open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 1000).unwrap();
        }
        let key = subject_key("pinky", "bluey", "preprod", "overflow");
        assert_domain(
            open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 1000).unwrap_err(),
            true,
        );
        assert!(
            crate::db::accounts::Account::fetch_by_id(&pool, &format!("pinky_{key}"))
                .unwrap()
                .is_none()
        );
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn concurrent_start_cannot_duplicate_ownership_or_exceed_active_limit() {
        let (pool, path) = pool();
        let key = subject_key("pinky", "bluey", "preprod", "a");
        let id = uuid::Uuid::new_v4().to_string();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (pool, key, id, barrier) =
                    (pool.clone(), key.clone(), id.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    open(&pool, &key, &id, 1000)
                })
            })
            .collect();
        for worker in workers {
            assert!(worker.join().unwrap().is_ok());
        }
        let count: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pinky_ai_sessions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let (pool, key, barrier) = (pool.clone(), key.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 1000)
                })
            })
            .collect();
        let mut accepted = 0;
        for worker in workers {
            match worker.join().unwrap() {
                Ok(_) => accepted += 1,
                Err(error) => assert_domain(error, true),
            }
        }
        assert_eq!(accepted, 2);
        drop(pool);
        std::fs::remove_file(path).unwrap();
    }
}
