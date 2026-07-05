//! Account record DB access. Stub for now; expanded in subsequent commits.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use postgres::error::SqlState;
use postgres::Row as PgRow;
use rusqlite::params;

use crate::db::DbPool;

const DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS: i64 = 500;
const DEFAULT_AUTO_TOPUP_AMOUNT_CENTS: i64 = 1500;
pub const DEFAULT_TRIAL_SECONDS: i64 = 15 * 60;

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
    #[error("postgres: {0}")]
    Postgres(#[from] postgres::Error),
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
    pub is_temporary: bool,
    pub temporary_expires_at: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingRiskAccountSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub billing_restriction_reason: Option<String>,
    pub billing_restricted_at: Option<String>,
    pub latest_ledger_event_type: Option<String>,
    pub latest_ledger_amount_cents: Option<i64>,
    pub latest_ledger_created_at: Option<String>,
}

impl Account {
    const SELECT_FIELDS: &'static str = "id, email, email_verified_at,
                    balance_cents, trial_seconds_remaining,
                    is_temporary, temporary_expires_at,
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
            is_temporary: r.get::<_, i64>(5)? == 1,
            temporary_expires_at: r.get(6)?,
            auto_topup_enabled: r.get::<_, i64>(7)? == 1,
            auto_topup_threshold_cents: r.get(8)?,
            auto_topup_amount_cents: r.get(9)?,
            is_admin: r.get::<_, i64>(10)? == 1,
            stripe_customer_id: r.get(11)?,
            stripe_payment_method_id: r.get(12)?,
            square_customer_id: r.get(13)?,
            square_card_id: r.get(14)?,
            square_card_brand: r.get(15)?,
            square_card_last4: r.get(16)?,
            billing_restricted: r.get::<_, i64>(17)? == 1,
            billing_restriction_reason: r.get(18)?,
            billing_restricted_at: r.get(19)?,
        })
    }

    fn from_pg_row(r: &PgRow) -> Result<Self> {
        let email_verified_at: Option<DateTime<Utc>> = r.try_get(2)?;
        let temporary_expires_at: Option<DateTime<Utc>> = r.try_get(6)?;
        let billing_restricted_at: Option<DateTime<Utc>> = r.try_get(19)?;
        Ok(Self {
            id: r.try_get(0)?,
            email: r.try_get(1)?,
            email_verified_at: email_verified_at.map(|value| value.to_rfc3339()),
            balance_cents: r.try_get(3)?,
            trial_seconds_remaining: r.try_get(4)?,
            is_temporary: r.try_get::<_, i32>(5)? != 0,
            temporary_expires_at: temporary_expires_at.map(|value| value.to_rfc3339()),
            auto_topup_enabled: r.try_get::<_, i32>(7)? != 0,
            auto_topup_threshold_cents: r.try_get(8)?,
            auto_topup_amount_cents: r.try_get(9)?,
            is_admin: r.try_get::<_, i32>(10)? != 0,
            stripe_customer_id: r.try_get(11)?,
            stripe_payment_method_id: r.try_get(12)?,
            square_customer_id: r.try_get(13)?,
            square_card_id: r.try_get(14)?,
            square_card_brand: r.try_get(15)?,
            square_card_last4: r.try_get(16)?,
            billing_restricted: r.try_get::<_, i32>(17)? != 0,
            billing_restriction_reason: r.try_get(18)?,
            billing_restricted_at: billing_restricted_at.map(|value| value.to_rfc3339()),
        })
    }

    pub fn is_temporary_expired(&self) -> bool {
        if !self.is_temporary {
            return false;
        }
        let Some(expires_at) = self.temporary_expires_at.as_deref() else {
            return true;
        };
        let Ok(expires_at) = expires_at.parse::<DateTime<Utc>>() else {
            return true;
        };
        expires_at <= Utc::now()
    }

    pub fn fetch_by_id(pool: &DbPool, id: &str) -> Result<Option<Self>> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut stmt = conn.prepare(&format!(
                    "SELECT {} FROM accounts WHERE id = ?1",
                    Self::SELECT_FIELDS
                ))?;
                let row = stmt.query_row(params![id], Self::from_row).ok();
                Ok(row)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    &format!("SELECT {} FROM accounts WHERE id = $1", Self::SELECT_FIELDS),
                    &[&id],
                )?;
                row.map(|row| Self::from_pg_row(&row)).transpose()
            }
        })
    }

    pub fn fetch_by_email(pool: &DbPool, email: &str) -> Result<Option<Self>> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut stmt = conn.prepare(&format!(
                    "SELECT {} FROM accounts WHERE email = ?1",
                    Self::SELECT_FIELDS
                ))?;
                let row = stmt.query_row(params![email], Self::from_row).ok();
                Ok(row)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    &format!(
                        "SELECT {} FROM accounts WHERE email = $1",
                        Self::SELECT_FIELDS
                    ),
                    &[&email],
                )?;
                row.map(|row| Self::from_pg_row(&row)).transpose()
            }
        })
    }

    pub fn create(pool: &DbPool, email: &str, password_hash: &str) -> Result<Self> {
        crate::db::run_blocking_db(|| Self::create_with_admin(pool, email, password_hash, false))
    }

    pub fn create_with_admin(
        pool: &DbPool,
        email: &str,
        password_hash: &str,
        is_admin: bool,
    ) -> Result<Self> {
        crate::db::run_blocking_db(|| {
            let id = uuid::Uuid::new_v4().to_string();
            match pool {
                DbPool::Sqlite(_) => {
                    let conn = pool.get()?;
                    match conn.execute(
                        "INSERT INTO accounts
                        (id, email, password_hash, is_admin, auto_topup_enabled,
                         auto_topup_threshold_cents, auto_topup_amount_cents,
                         trial_seconds_remaining)
                     VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7)",
                        params![
                            id,
                            email,
                            password_hash,
                            if is_admin { 1 } else { 0 },
                            DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS,
                            DEFAULT_AUTO_TOPUP_AMOUNT_CENTS,
                            DEFAULT_TRIAL_SECONDS
                        ],
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
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let is_admin_i32 = if is_admin { 1i32 } else { 0i32 };
                    if let Err(e) = conn.execute(
                        "INSERT INTO accounts
                        (id, email, password_hash, is_admin, auto_topup_enabled,
                         auto_topup_threshold_cents, auto_topup_amount_cents,
                         trial_seconds_remaining)
                     VALUES ($1, $2, $3, $4, 0, $5, $6, $7)",
                        &[
                            &id,
                            &email,
                            &password_hash,
                            &is_admin_i32,
                            &DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS,
                            &DEFAULT_AUTO_TOPUP_AMOUNT_CENTS,
                            &DEFAULT_TRIAL_SECONDS,
                        ],
                    ) {
                        if e.code() == Some(&SqlState::UNIQUE_VIOLATION) {
                            return Err(AccountCreateError::DuplicateEmail.into());
                        }
                        return Err(AccountCreateError::Postgres(e).into());
                    }
                }
            }
            Ok(Self {
                id,
                email: email.to_string(),
                email_verified_at: None,
                balance_cents: 0,
                trial_seconds_remaining: DEFAULT_TRIAL_SECONDS,
                is_temporary: false,
                temporary_expires_at: None,
                auto_topup_enabled: false,
                auto_topup_threshold_cents: DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS,
                auto_topup_amount_cents: DEFAULT_AUTO_TOPUP_AMOUNT_CENTS,
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
        })
    }

    pub fn create_temporary(
        pool: &DbPool,
        email: &str,
        password_hash: &str,
        trial_seconds_remaining: i64,
        temporary_expires_at: &str,
    ) -> Result<Self> {
        crate::db::run_blocking_db(|| {
            let id = uuid::Uuid::new_v4().to_string();
            let trial_seconds_remaining = trial_seconds_remaining.max(0);
            match pool {
                DbPool::Sqlite(_) => {
                    let conn = pool.get()?;
                    match conn.execute(
                        "INSERT INTO accounts
                        (id, email, password_hash, email_verified_at,
                         trial_seconds_remaining, is_temporary, temporary_expires_at,
                         auto_topup_enabled, auto_topup_threshold_cents, auto_topup_amount_cents)
                     VALUES (?1, ?2, ?3, datetime('now'), ?4, 1, ?5, 0, ?6, ?7)",
                        params![
                            id,
                            email,
                            password_hash,
                            trial_seconds_remaining,
                            temporary_expires_at,
                            DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS,
                            DEFAULT_AUTO_TOPUP_AMOUNT_CENTS
                        ],
                    ) {
                        Ok(_) => {}
                        Err(e) => {
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
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let temporary_expires_at =
                        chrono::DateTime::parse_from_rfc3339(temporary_expires_at)?
                            .with_timezone(&Utc);
                    if let Err(e) = conn.execute(
                        "INSERT INTO accounts
                        (id, email, password_hash, email_verified_at,
                         trial_seconds_remaining, is_temporary, temporary_expires_at,
                         auto_topup_enabled, auto_topup_threshold_cents, auto_topup_amount_cents)
                     VALUES ($1, $2, $3, now(), $4, 1, $5, 0, $6, $7)",
                        &[
                            &id,
                            &email,
                            &password_hash,
                            &trial_seconds_remaining,
                            &temporary_expires_at,
                            &DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS,
                            &DEFAULT_AUTO_TOPUP_AMOUNT_CENTS,
                        ],
                    ) {
                        if e.code() == Some(&SqlState::UNIQUE_VIOLATION) {
                            return Err(AccountCreateError::DuplicateEmail.into());
                        }
                        return Err(AccountCreateError::Postgres(e).into());
                    }
                }
            }
            Ok(Self {
                id,
                email: email.to_string(),
                email_verified_at: Some(Utc::now().to_rfc3339()),
                balance_cents: 0,
                trial_seconds_remaining,
                is_temporary: true,
                temporary_expires_at: Some(temporary_expires_at.to_string()),
                auto_topup_enabled: false,
                auto_topup_threshold_cents: DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS,
                auto_topup_amount_cents: DEFAULT_AUTO_TOPUP_AMOUNT_CENTS,
                is_admin: false,
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
        })
    }

    pub fn convert_temporary_to_registered(
        pool: &DbPool,
        id: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<Option<Self>> {
        crate::db::run_blocking_db(|| {
            let changed = match pool {
                DbPool::Sqlite(_) => {
                    let conn = pool.get()?;
                    match conn.execute(
                        "UPDATE accounts
                            SET email = ?2,
                                password_hash = ?3,
                                email_verified_at = datetime('now'),
                                is_temporary = 0,
                                temporary_expires_at = NULL
                          WHERE id = ?1
                            AND is_temporary = 1",
                        params![id, email, password_hash],
                    ) {
                        Ok(changed) => changed,
                        Err(e) => {
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
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    match conn.execute(
                        "UPDATE accounts
                            SET email = $2,
                                password_hash = $3,
                                email_verified_at = now(),
                                is_temporary = 0,
                                temporary_expires_at = NULL
                          WHERE id = $1
                            AND is_temporary = 1",
                        &[&id, &email, &password_hash],
                    ) {
                        Ok(changed) => usize::try_from(changed).unwrap_or(usize::MAX),
                        Err(e) => {
                            if e.code() == Some(&SqlState::UNIQUE_VIOLATION) {
                                return Err(AccountCreateError::DuplicateEmail.into());
                            }
                            return Err(AccountCreateError::Postgres(e).into());
                        }
                    }
                }
            };
            if changed == 0 {
                return Ok(None);
            }
            Self::fetch_by_id(pool, id)
        })
    }

    pub fn update_auto_topup_settings(
        pool: &DbPool,
        id: &str,
        enabled: bool,
        threshold_cents: i64,
        amount_cents: i64,
    ) -> Result<Option<Self>> {
        crate::db::run_blocking_db(|| {
            match pool {
                DbPool::Sqlite(_) => {
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
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let enabled_i32 = if enabled { 1i32 } else { 0i32 };
                    conn.execute(
                        "UPDATE accounts
                        SET auto_topup_enabled = $2,
                            auto_topup_threshold_cents = $3,
                            auto_topup_amount_cents = $4
                      WHERE id = $1",
                        &[&id, &enabled_i32, &threshold_cents, &amount_cents],
                    )?;
                }
            }
            Self::fetch_by_id(pool, id)
        })
    }

    pub fn save_square_card(
        pool: &DbPool,
        id: &str,
        customer_id: &str,
        card_id: &str,
        card_brand: Option<&str>,
        card_last4: Option<&str>,
    ) -> Result<Option<Self>> {
        crate::db::run_blocking_db(|| {
            match pool {
                DbPool::Sqlite(_) => {
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
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    conn.execute(
                        "UPDATE accounts
                        SET square_customer_id = $2,
                            square_card_id = $3,
                            square_card_brand = $4,
                            square_card_last4 = $5
                      WHERE id = $1",
                        &[&id, &customer_id, &card_id, &card_brand, &card_last4],
                    )?;
                }
            }
            Self::fetch_by_id(pool, id)
        })
    }

    pub fn save_stripe_checkout_refs(
        pool: &DbPool,
        id: &str,
        customer_id: Option<&str>,
        payment_method_id: Option<&str>,
    ) -> Result<usize> {
        crate::db::run_blocking_db(|| {
            if customer_id.is_none() && payment_method_id.is_none() {
                return Ok(0);
            }
            match pool {
                DbPool::Sqlite(_) => {
                    let conn = pool.get()?;
                    Ok(conn.execute(
                        "UPDATE accounts
                        SET stripe_customer_id = COALESCE(?2, stripe_customer_id),
                            stripe_payment_method_id = COALESCE(?3, stripe_payment_method_id)
                      WHERE id = ?1",
                        params![id, customer_id, payment_method_id],
                    )?)
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let affected = conn.execute(
                        "UPDATE accounts
                        SET stripe_customer_id = COALESCE($2, stripe_customer_id),
                            stripe_payment_method_id = COALESCE($3, stripe_payment_method_id)
                      WHERE id = $1",
                        &[&id, &customer_id, &payment_method_id],
                    )?;
                    Ok(usize::try_from(affected).unwrap_or(usize::MAX))
                }
            }
        })
    }

    pub fn set_admin(pool: &DbPool, id: &str, is_admin: bool) -> Result<()> {
        crate::db::run_blocking_db(|| {
            match pool {
                DbPool::Sqlite(_) => {
                    let conn = pool.get()?;
                    conn.execute(
                        "UPDATE accounts SET is_admin = ?2 WHERE id = ?1",
                        params![id, if is_admin { 1 } else { 0 }],
                    )?;
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let is_admin_i32 = if is_admin { 1i32 } else { 0i32 };
                    conn.execute(
                        "UPDATE accounts SET is_admin = $2 WHERE id = $1",
                        &[&id, &is_admin_i32],
                    )?;
                }
            }
            Ok(())
        })
    }

    pub fn mark_email_verified(pool: &DbPool, id: &str) -> Result<usize> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                Ok(conn.execute(
                    "UPDATE accounts SET email_verified_at = datetime('now') WHERE id = ?1",
                    params![id],
                )?)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let affected = conn.execute(
                    "UPDATE accounts SET email_verified_at = now() WHERE id = $1",
                    &[&id],
                )?;
                Ok(usize::try_from(affected).unwrap_or(usize::MAX))
            }
        })
    }

    pub fn update_password_hash(pool: &DbPool, id: &str, password_hash: &str) -> Result<usize> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                Ok(conn.execute(
                    "UPDATE accounts SET password_hash = ?1 WHERE id = ?2",
                    params![password_hash, id],
                )?)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let affected = conn.execute(
                    "UPDATE accounts SET password_hash = $1 WHERE id = $2",
                    &[&password_hash, &id],
                )?;
                Ok(usize::try_from(affected).unwrap_or(usize::MAX))
            }
        })
    }

    pub fn restrict_billing(
        pool: &DbPool,
        id: &str,
        reason: &str,
        event_id: Option<&str>,
    ) -> Result<Option<Self>> {
        crate::db::run_blocking_db(|| {
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
            match pool {
                DbPool::Sqlite(_) => {
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
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    conn.execute(
                        "UPDATE accounts
                        SET billing_restricted = 1,
                            billing_restriction_reason = $2,
                            billing_restricted_at = now(),
                            auto_topup_enabled = 0,
                            stripe_payment_method_id = NULL,
                            square_card_id = NULL,
                            square_card_brand = NULL,
                            square_card_last4 = NULL
                      WHERE id = $1",
                        &[&id, &reason_with_event],
                    )?;
                }
            }
            Self::fetch_by_id(pool, id)
        })
    }

    pub fn account_id_for_processor_payment(
        pool: &DbPool,
        provider: &str,
        processor_payment_id: &str,
    ) -> Result<Option<String>> {
        crate::db::run_blocking_db(|| {
            let provider = provider.trim().to_ascii_lowercase();
            let processor_payment_id = processor_payment_id.trim();
            if provider.is_empty() || processor_payment_id.is_empty() {
                return Ok(None);
            }
            let source_id = format!("{provider}:{processor_payment_id}");
            match pool {
                DbPool::Sqlite(_) => {
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
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let row = conn.query_opt(
                        "SELECT account_id FROM credit_batches WHERE stripe_charge_id = $1",
                        &[&source_id],
                    )?;
                    row.map(|row| row.try_get(0).context("read processor account id"))
                        .transpose()
                }
            }
        })
    }

    /// Look up password hash for login validation.
    pub fn password_hash(pool: &DbPool, email: &str) -> Result<Option<String>> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut stmt =
                    conn.prepare("SELECT password_hash FROM accounts WHERE email = ?1")?;
                let hash = stmt
                    .query_row(params![email], |r| r.get::<_, String>(0))
                    .ok();
                Ok(hash)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    "SELECT password_hash FROM accounts WHERE email = $1",
                    &[&email],
                )?;
                row.map(|row| row.try_get(0).context("read password hash"))
                    .transpose()
            }
        })
    }

    pub fn list_customer_summaries(pool: &DbPool, limit: i64) -> Result<Vec<CustomerSummary>> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
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
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let limit = limit.max(1);
                conn.query(
                    "SELECT id, email, balance_cents
                       FROM accounts
                      ORDER BY created_at DESC
                      LIMIT $1",
                    &[&limit],
                )?
                .into_iter()
                .map(|row| {
                    Ok(CustomerSummary {
                        id: row.try_get(0)?,
                        email: row.try_get(1)?,
                        balance_cents: row.try_get(2)?,
                    })
                })
                .collect()
            }
        })
    }

    pub fn list_billing_risk_summaries(
        pool: &DbPool,
        limit: i64,
    ) -> Result<Vec<BillingRiskAccountSummary>> {
        crate::db::run_blocking_db(|| match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut stmt = conn.prepare(
                    "SELECT
                        a.id,
                        a.email,
                        a.balance_cents,
                        a.billing_restriction_reason,
                        a.billing_restricted_at,
                        (
                            SELECT b.event_type
                              FROM balance_ledger_entries b
                             WHERE b.account_id = a.id
                             ORDER BY b.created_at DESC
                             LIMIT 1
                        ) AS latest_event_type,
                        (
                            SELECT b.amount_cents
                              FROM balance_ledger_entries b
                             WHERE b.account_id = a.id
                             ORDER BY b.created_at DESC
                             LIMIT 1
                        ) AS latest_amount_cents,
                        (
                            SELECT b.created_at
                              FROM balance_ledger_entries b
                             WHERE b.account_id = a.id
                             ORDER BY b.created_at DESC
                             LIMIT 1
                        ) AS latest_created_at
                       FROM accounts a
                      WHERE a.billing_restricted = 1
                      ORDER BY a.billing_restricted_at DESC
                      LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit.max(1)], |row| {
                    Ok(BillingRiskAccountSummary {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        balance_cents: row.get(2)?,
                        billing_restriction_reason: row.get(3)?,
                        billing_restricted_at: row.get(4)?,
                        latest_ledger_event_type: row.get(5)?,
                        latest_ledger_amount_cents: row.get(6)?,
                        latest_ledger_created_at: row.get(7)?,
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let limit = limit.max(1);
                conn.query(
                    "SELECT
                        a.id,
                        a.email,
                        a.balance_cents,
                        a.billing_restriction_reason,
                        a.billing_restricted_at,
                        latest.event_type,
                        latest.amount_cents,
                        latest.created_at
                       FROM accounts a
                       LEFT JOIN LATERAL (
                            SELECT event_type, amount_cents, created_at
                              FROM balance_ledger_entries
                             WHERE account_id = a.id
                             ORDER BY created_at DESC
                             LIMIT 1
                       ) latest ON TRUE
                      WHERE a.billing_restricted != 0
                      ORDER BY a.billing_restricted_at DESC
                      LIMIT $1",
                    &[&limit],
                )?
                .into_iter()
                .map(|row| {
                    let billing_restricted_at: Option<DateTime<Utc>> = row.try_get(4)?;
                    let latest_created_at: Option<DateTime<Utc>> = row.try_get(7)?;
                    Ok(BillingRiskAccountSummary {
                        id: row.try_get(0)?,
                        email: row.try_get(1)?,
                        balance_cents: row.try_get(2)?,
                        billing_restriction_reason: row.try_get(3)?,
                        billing_restricted_at: billing_restricted_at
                            .map(|value| value.to_rfc3339()),
                        latest_ledger_event_type: row.try_get(5)?,
                        latest_ledger_amount_cents: row.try_get(6)?,
                        latest_ledger_created_at: latest_created_at.map(|value| value.to_rfc3339()),
                    })
                })
                .collect()
            }
        })
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

    #[test]
    fn create_initializes_fifteen_minute_trial_budget() {
        let pool = temp_pool();
        let account = Account::create(&pool, "trial@example.com", "hash").unwrap();
        assert_eq!(account.trial_seconds_remaining, DEFAULT_TRIAL_SECONDS);

        let stored = Account::fetch_by_email(&pool, "trial@example.com")
            .unwrap()
            .expect("account should be persisted");
        assert_eq!(stored.trial_seconds_remaining, DEFAULT_TRIAL_SECONDS);
        assert_eq!(
            stored.auto_topup_threshold_cents,
            DEFAULT_AUTO_TOPUP_THRESHOLD_CENTS
        );
        assert_eq!(
            stored.auto_topup_amount_cents,
            DEFAULT_AUTO_TOPUP_AMOUNT_CENTS
        );
    }

    #[test]
    fn billing_risk_summary_includes_latest_balance_ledger_event() {
        let pool = temp_pool();
        let account = Account::create(&pool, "risk@example.com", "hash").unwrap();
        crate::db::balance::credit_processor_payment(
            &pool,
            &account.id,
            3000,
            "square",
            "pay_risk",
        )
        .unwrap();
        Account::restrict_billing(&pool, &account.id, "refund.created", Some("square:evt_1"))
            .unwrap();

        let rows = Account::list_billing_risk_summaries(&pool, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].email, "risk@example.com");
        assert_eq!(
            rows[0].billing_restriction_reason.as_deref(),
            Some("refund.created:square:evt_1")
        );
        assert_eq!(
            rows[0].latest_ledger_event_type.as_deref(),
            Some("processor_payment_credit")
        );
        assert_eq!(rows[0].latest_ledger_amount_cents, Some(3000));
    }
}
