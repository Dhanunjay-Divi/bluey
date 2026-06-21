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
    pub email_verified_at: Option<String>,
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
    /// Square customer id used for card-on-file auto reload.
    pub square_customer_id: Option<String>,
    /// Square card id returned by the Cards API. Never raw card data.
    pub square_card_id: Option<String>,
    pub square_card_brand: Option<String>,
    pub square_card_last4: Option<String>,
    pub billing_restricted: bool,
    pub billing_restriction_reason: Option<String>,
    pub billing_restricted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
}

impl Account {
    const SELECT_FIELDS: &'static str = "id, email, email_verified_at,
                    balance_cents, trial_seconds_remaining,
                    auto_topup_enabled, auto_topup_threshold_cents,
                    auto_topup_amount_cents, is_admin,
                    stripe_customer_id, stripe_payment_method_id,
                    square_customer_id, square_card_id,
                    square_card_brand, square_card_last4,
                    billing_restricted, billing_restriction_reason,
                    billing_restricted_at";

    fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            email: r.get(1)?,
            email_verified_at: r.get(2)?,
            balance_cents: r.get(3)?,
            trial_seconds_remaining: r.get(4)?,
            auto_topup_enabled: r.get::<_, i64>(5)? == 1,
            auto_topup_threshold_cents: r.get(6)?,
            auto_topup_amount_cents: r.get(7)?,
            is_admin: r.get::<_, i64>(8)? == 1,
            stripe_customer_id: r.get(9)?,
            stripe_payment_method_id: r.get(10)?,
            square_customer_id: r.get(11)?,
            square_card_id: r.get(12)?,
            square_card_brand: r.get(13)?,
            square_card_last4: r.get(14)?,
            billing_restricted: r.get::<_, i64>(15)? == 1,
            billing_restriction_reason: r.get(16)?,
            billing_restricted_at: r.get(17)?,
        })
    }

    pub fn fetch_by_id(pool: &DbPool, id: &str) -> Result<Option<Self>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM accounts WHERE id = ?1",
            Self::SELECT_FIELDS
        ))?;
        let row = stmt
            .query_row(params![id], Self::from_row)
            .ok();
        Ok(row)
    }

    pub fn fetch_by_email(pool: &DbPool, email: &str) -> Result<Option<Self>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM accounts WHERE email = ?1",
            Self::SELECT_FIELDS
        ))?;
        let row = stmt
            .query_row(params![email], Self::from_row)
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
            "INSERT INTO accounts
                (id, email, password_hash, is_admin, auto_topup_enabled)
             VALUES (?1, ?2, ?3, ?4, 0)",
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
            email_verified_at: None,
            balance_cents: 0,
            trial_seconds_remaining: 600,
            auto_topup_enabled: false,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 1500,
            is_admin,
            stripe_customer_id: None,
            stripe_payment_method_id: None,
            square_customer_id: None,
            square_card_id: None,
            square_card_brand: None,
            square_card_last4: None,
            billing_restricted: false,
            billing_restriction_reason: None,
            billing_restricted_at: None,
        })
    }

    pub fn update_auto_topup_settings(
        pool: &DbPool,
        id: &str,
        enabled: bool,
        threshold_cents: i64,
        amount_cents: i64,
    ) -> Result<Option<Self>> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE accounts
                SET auto_topup_enabled = ?2,
                    auto_topup_threshold_cents = ?3,
                    auto_topup_amount_cents = ?4
              WHERE id = ?1",
            params![
                id,
                if enabled { 1 } else { 0 },
                threshold_cents,
                amount_cents
            ],
        )?;
        drop(conn);
        Self::fetch_by_id(pool, id)
    }

    pub fn save_square_card(
        pool: &DbPool,
        id: &str,
        customer_id: &str,
        card_id: &str,
        card_brand: Option<&str>,
        card_last4: Option<&str>,
    ) -> Result<Option<Self>> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE accounts
                SET square_customer_id = ?2,
                    square_card_id = ?3,
                    square_card_brand = ?4,
                    square_card_last4 = ?5
              WHERE id = ?1",
            params![id, customer_id, card_id, card_brand, card_last4],
        )?;
        drop(conn);
        Self::fetch_by_id(pool, id)
    }

    pub fn save_stripe_checkout_refs(
        pool: &DbPool,
        id: &str,
        customer_id: Option<&str>,
        payment_method_id: Option<&str>,
    ) -> Result<usize> {
        if customer_id.is_none() && payment_method_id.is_none() {
            return Ok(0);
        }
        let conn = pool.get()?;
        Ok(conn.execute(
            "UPDATE accounts
                SET stripe_customer_id = COALESCE(?2, stripe_customer_id),
                    stripe_payment_method_id = COALESCE(?3, stripe_payment_method_id)
              WHERE id = ?1",
            params![id, customer_id, payment_method_id],
        )?)
    }

    pub fn set_admin(pool: &DbPool, id: &str, is_admin: bool) -> Result<()> {
        let conn = pool.get()?;
        conn.execute(
            "UPDATE accounts SET is_admin = ?2 WHERE id = ?1",
            params![id, if is_admin { 1 } else { 0 }],
        )?;
        Ok(())
    }

    pub fn mark_email_verified(pool: &DbPool, id: &str) -> Result<usize> {
        let conn = pool.get()?;
        Ok(conn.execute(
            "UPDATE accounts SET email_verified_at = datetime('now') WHERE id = ?1",
            params![id],
        )?)
    }

    pub fn update_password_hash(pool: &DbPool, id: &str, password_hash: &str) -> Result<usize> {
        let conn = pool.get()?;
        Ok(conn.execute(
            "UPDATE accounts SET password_hash = ?1 WHERE id = ?2",
            params![password_hash, id],
        )?)
    }

    pub fn restrict_billing(
        pool: &DbPool,
        id: &str,
        reason: &str,
        event_id: Option<&str>,
    ) -> Result<Option<Self>> {
        let reason = reason.trim();
        let reason = if reason.is_empty() {
            "billing_review"
        } else {
            reason
        };
        let reason_with_event = event_id
            .filter(|value| !value.trim().is_empty())
            .map(|event_id| format!("{reason}:{event_id}"))
            .unwrap_or_else(|| reason.to_string());
        let conn = pool.get()?;
        conn.execute(
            "UPDATE accounts
                SET billing_restricted = 1,
                    billing_restriction_reason = ?2,
                    billing_restricted_at = datetime('now'),
                    auto_topup_enabled = 0,
                    stripe_payment_method_id = NULL,
                    square_card_id = NULL,
                    square_card_brand = NULL,
                    square_card_last4 = NULL
              WHERE id = ?1",
            params![id, reason_with_event],
        )?;
        drop(conn);
        Self::fetch_by_id(pool, id)
    }

    pub fn account_id_for_processor_payment(
        pool: &DbPool,
        provider: &str,
        processor_payment_id: &str,
    ) -> Result<Option<String>> {
        let provider = provider.trim().to_ascii_lowercase();
        let processor_payment_id = processor_payment_id.trim();
        if provider.is_empty() || processor_payment_id.is_empty() {
            return Ok(None);
        }
        let source_id = format!("{provider}:{processor_payment_id}");
        let conn = pool.get()?;
        let account_id = conn
            .query_row(
                "SELECT account_id FROM credit_batches WHERE stripe_charge_id = ?1",
                params![source_id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        Ok(account_id)
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

    pub fn list_customer_summaries(pool: &DbPool, limit: i64) -> Result<Vec<CustomerSummary>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, email, balance_cents
               FROM accounts
              ORDER BY created_at DESC
              LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.max(1)], |row| {
            Ok(CustomerSummary {
                id: row.get(0)?,
                email: row.get(1)?,
                balance_cents: row.get(2)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
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
