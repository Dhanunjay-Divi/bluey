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
"#;

const MAX_BINDINGS: i64 = 100;
const MAX_SESSIONS: i64 = 10_000;
const MAX_SESSIONS_PER_BINDING: i64 = 1_000;
const MAX_ACTIVE_PER_BINDING: i64 = 3;
const EXTERNAL_PASSWORD_SENTINEL: &str = "!external-pinky-v1";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("integration request denied")]
    Denied,
    #[error("integration preprod capacity reached")]
    Capacity,
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
    pub expires_at: i64,
}

fn session(id: &str, state: String, expires_at: i64, now: i64) -> AiSession {
    AiSession {
        session_id: id.to_owned(),
        state: if state == "active" && expires_at <= now {
            "expired".into()
        } else {
            state
        },
        expires_at,
    }
}

/// Trusted Pinky may provision only a distinct external-only account with zero
/// credits/trial/admin/reload. Email equality never links an existing wallet.
/// Account, binding and session are committed atomically. Closed/expired UUIDs
/// cannot be reopened by replay; a new Start needs a new UUID.
pub fn open(pool: &DbPool, key: &str, id: &str, now: i64) -> Result<AiSession> {
    let expiry = now.checked_add(3600).ok_or(StoreError::Denied)?;
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
                tx.commit()?;
                return Ok(session(id, state, expires_at, now));
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
            tx.commit()?;
            Ok(session(id, "active".into(), expiry, now))
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
                let result = session(id, row.get(1), row.get(2), now);
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
            let result = session(id, row.get(1), row.get(2), now);
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
            let row = conn
                .query_row(
                    "UPDATE pinky_ai_sessions SET state='closed',closed_at=COALESCE(closed_at,?3)
                     WHERE id=?1 AND subject_key=?2 RETURNING expires_at",
                    params![id, key, now],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            Ok(row.map(|expiry| session(id, "closed".into(), expiry, now)))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "UPDATE pinky_ai_sessions SET state='closed',closed_at=COALESCE(closed_at,$3)
                 WHERE id=$1 AND subject_key=$2 RETURNING expires_at",
                &[&id, &key, &now],
            )?;
            Ok(row.map(|row| session(id, "closed".into(), row.get(0), now)))
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
        assert_eq!(created, open(&pool, &key, &id, 1100).unwrap());
        assert!(open(&pool, &other, &id, 1100).is_err());
        assert!(close(&pool, &other, &id, 1100).unwrap().is_none());
        close(&pool, &key, &id, 1200).unwrap().unwrap();
        assert_eq!(open(&pool, &key, &id, 1300).unwrap().state, "closed");
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
        assert_eq!(open(&pool, &key, &id, 4600).unwrap().state, "expired");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE pinky_ai_bindings SET revoked_at=2000 WHERE subject_key=?1",
                [&key],
            )
            .unwrap();
        assert!(open(&pool, &key, &uuid::Uuid::new_v4().to_string(), 2100).is_err());
        assert!(close(&pool, &key, &id, 2100).unwrap().is_some());
        drop(pool);
        std::fs::remove_file(path).unwrap();
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
