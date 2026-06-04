//! Account record DB access. Stub for now; expanded in subsequent commits.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

/// Codex Stage 10 round-2 typed-error nit: signup needs to distinguish
/// "this email already exists" from generic DB errors without
/// string-matching error messages. AccountCreateError is the typed
/// variant; it is downcast via anyhow at the handler boundary.
#[derive(Debug, thiserror::Error)]
pub enum AccountCreateError {
    #[error("duplicate-email")]
    DuplicateEmail,
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("pool: {0}")]
    Pool(#[from] r2d2::Error),
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
    pub is_admin: bool,
    /// Codex Stage 10: present once the customer has completed their
    /// first Stripe Checkout. Used as the customer reference for
    /// off-session auto top-up charges.
    pub stripe_customer_id: Option<String>,
    /// Codex Stage 10: present once the PaymentMethod has been
    /// retrieved (Stage 6 round-2 fix). Used as the saved card for
    /// off-session auto top-up.
    pub stripe_payment_method_id: Option<String>,
}

impl Account {
    pub fn fetch_by_id(pool: &DbPool, id: &str) -> Result<Option<Self>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    auto_topup_enabled, auto_topup_threshold_cents,
                    auto_topup_amount_cents, is_admin,
                    stripe_customer_id, stripe_payment_method_id
             FROM accounts WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![id], |r| {
                Ok(Self {
                    id: r.get(0)?,
                    email: r.get(1)?,
                    balance_cents: r.get(2)?,
                    trial_seconds_remaining: r.get(3)?,
                    auto_topup_enabled: r.get::<_, i64>(4)? == 1,
                    auto_topup_threshold_cents: r.get(5)?,
                    auto_topup_amount_cents: r.get(6)?,
                    is_admin: r.get::<_, i64>(7)? == 1,
                    stripe_customer_id: r.get(8)?,
                    stripe_payment_method_id: r.get(9)?,
                })
            })
            .ok();
        Ok(row)
    }

    pub fn fetch_by_email(pool: &DbPool, email: &str) -> Result<Option<Self>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    auto_topup_enabled, auto_topup_threshold_cents,
                    auto_topup_amount_cents, is_admin,
                    stripe_customer_id, stripe_payment_method_id
             FROM accounts WHERE email = ?1",
        )?;
        let row = stmt
            .query_row(params![email], |r| {
                Ok(Self {
                    id: r.get(0)?,
                    email: r.get(1)?,
                    balance_cents: r.get(2)?,
                    trial_seconds_remaining: r.get(3)?,
                    auto_topup_enabled: r.get::<_, i64>(4)? == 1,
                    auto_topup_threshold_cents: r.get(5)?,
                    auto_topup_amount_cents: r.get(6)?,
                    is_admin: r.get::<_, i64>(7)? == 1,
                    stripe_customer_id: r.get(8)?,
                    stripe_payment_method_id: r.get(9)?,
                })
            })
            .ok();
        Ok(row)
    }

    pub fn create(pool: &DbPool, email: &str, password_hash: &str) -> Result<Self> {
        Self::create_with_admin(pool, email, password_hash, false)
    }

    pub fn create_with_admin(
        pool: &DbPool,
        email: &str,
        password_hash: &str,
        is_admin: bool,
    ) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let conn = pool.get()?;
        match conn.execute(
            "INSERT INTO accounts (id, email, password_hash, is_admin) VALUES (?1, ?2, ?3, ?4)",
            params![id, email, password_hash, if is_admin { 1 } else { 0 }],
        ) {
            Ok(_) => {}
            Err(e) => {
                // Codex Stage 10 round-2 typed-error nit: emit the typed
                // AccountCreateError::DuplicateEmail (wrapped in anyhow)
                // so the signup handler can downcast cleanly. Generic
                // DB errors propagate as AccountCreateError::Db.
                if let rusqlite::Error::SqliteFailure(ref ff, ref msg) = e {
                    let is_unique = ff.code == rusqlite::ErrorCode::ConstraintViolation
                        && msg
                            .as_deref()
                            .map(|m| m.to_lowercase().contains("unique"))
                            .unwrap_or(false);
                    if is_unique {
                        return Err(AccountCreateError::DuplicateEmail.into());
                    }
                }
                return Err(AccountCreateError::Db(e).into());
            }
        }
        Ok(Self {
            id,
            email: email.to_string(),
            balance_cents: 0,
            trial_seconds_remaining: 600,
            auto_topup_enabled: true,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 3000,
            is_admin,
            stripe_customer_id: None,
            stripe_payment_method_id: None,
        })
    }

    pub fn set_admin(pool: &DbPool, id: &str, is_admin: bool) -> Result<()> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE accounts SET is_admin = ?2 WHERE id = ?1",
            params![id, if is_admin { 1 } else { 0 }],
        )?;
        Ok(())
    }

    /// Look up password hash for login validation.
    pub fn password_hash(pool: &DbPool, email: &str) -> Result<Option<String>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare("SELECT password_hash FROM accounts WHERE email = ?1")?;
        let hash = stmt
            .query_row(params![email], |r| r.get::<_, String>(0))
            .ok();
        Ok(hash)
    }
}
#[cfg(test)]
mod create_dup_tests {
    use super::*;
    use crate::db::{open_pool, run_migrations, DbPool};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-acc-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn create_returns_duplicate_email_marker_on_unique_violation() {
        // Codex Stage 10 (S2.4): atomic UNIQUE-violation -> typed marker.
        let pool = temp_pool();
        Account::create(&pool, "dup@example.com", "hash1").unwrap();
        let err = Account::create(&pool, "dup@example.com", "hash2").unwrap_err();
        // Codex Stage 10 round-2: assert via typed downcast, not string match.
        assert!(
            matches!(
                err.downcast_ref::<AccountCreateError>(),
                Some(AccountCreateError::DuplicateEmail)
            ),
            "expected AccountCreateError::DuplicateEmail, got: {err}"
        );
    }
}
