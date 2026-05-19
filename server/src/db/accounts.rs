//! Account record DB access. Stub for now; expanded in subsequent commits.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

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
}

impl Account {
    pub fn fetch_by_id(pool: &DbPool, id: &str) -> Result<Option<Self>> {
        let conn = pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    auto_topup_enabled, auto_topup_threshold_cents,
                    auto_topup_amount_cents, is_admin
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
                    auto_topup_amount_cents, is_admin
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
                })
            })
            .ok();
        Ok(row)
    }

    pub fn create(pool: &DbPool, email: &str, password_hash: &str) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let conn = pool.get()?;
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash) VALUES (?1, ?2, ?3)",
            params![id, email, password_hash],
        )?;
        Ok(Self {
            id,
            email: email.to_string(),
            balance_cents: 0,
            trial_seconds_remaining: 600,
            auto_topup_enabled: true,
            auto_topup_threshold_cents: 500,
            auto_topup_amount_cents: 3000,
            is_admin: false,
        })
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
