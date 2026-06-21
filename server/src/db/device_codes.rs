//! Device-code login storage. Browser/device auth routes use this module
//! instead of issuing SQLite directly.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCodeRow {
    pub account_id: Option<String>,
    pub approved: i64,
    pub expires_at: String,
}

pub fn insert(
    pool: &DbPool,
    device_code: &str,
    user_code: &str,
    expires_at: &str,
) -> Result<()> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO device_codes (device_code, user_code, expires_at) VALUES (?1, ?2, ?3)",
                params![device_code, user_code, expires_at],
            )?;
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO device_codes (device_code, user_code, expires_at) VALUES ($1, $2, $3::timestamptz)",
                &[&device_code, &user_code, &expires_at],
            )?;
        }
    }
    Ok(())
}

pub fn fetch_by_device_code(pool: &DbPool, device_code: &str) -> Result<Option<DeviceCodeRow>> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let row = conn
                .query_row(
                    "SELECT account_id, approved, expires_at FROM device_codes WHERE device_code = ?1",
                    params![device_code],
                    |r| {
                        Ok(DeviceCodeRow {
                            account_id: r.get(0)?,
                            approved: r.get(1)?,
                            expires_at: r.get(2)?,
                        })
                    },
                )
                .ok();
            Ok(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT account_id, approved, expires_at FROM device_codes WHERE device_code = $1",
                &[&device_code],
            )?;
            row.map(|row| {
                let expires_at: chrono::DateTime<chrono::Utc> = row.try_get(2)?;
                Ok(DeviceCodeRow {
                    account_id: row.try_get(0)?,
                    approved: row.try_get::<_, i32>(1)? as i64,
                    expires_at: expires_at.to_rfc3339(),
                })
            })
            .transpose()
        }
    }
}

pub fn consume_approved(pool: &DbPool, device_code: &str, account_id: &str) -> Result<bool> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let deleted = conn.execute(
                "DELETE FROM device_codes
                 WHERE device_code = ?1 AND approved = 1 AND account_id = ?2",
                params![device_code, account_id],
            )?;
            Ok(deleted > 0)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let deleted = conn.execute(
                "DELETE FROM device_codes
                 WHERE device_code = $1 AND approved = 1 AND account_id = $2",
                &[&device_code, &account_id],
            )?;
            Ok(deleted > 0)
        }
    }
}

pub fn approve_user_code(
    pool: &DbPool,
    account_id: &str,
    user_code: &str,
    now: &str,
) -> Result<bool> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let updated = conn.execute(
                "UPDATE device_codes SET approved = 1, account_id = ?1
                 WHERE user_code = ?2 AND expires_at > ?3 AND approved = 0",
                params![account_id, user_code, now],
            )?;
            Ok(updated > 0)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let updated = conn.execute(
                "UPDATE device_codes SET approved = 1, account_id = $1
                 WHERE user_code = $2 AND expires_at > $3::timestamptz AND approved = 0",
                &[&account_id, &user_code, &now],
            )?;
            Ok(updated > 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-devcode-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool) -> String {
        crate::db::accounts::Account::create(pool, "device@example.com", "stub")
            .unwrap()
            .id
    }

    #[test]
    fn approve_then_consume_is_single_use() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        insert(&pool, "device-1", "USER-123", "2099-01-01T00:00:00Z").unwrap();
        assert!(
            approve_user_code(&pool, &account_id, "USER-123", "2026-01-01T00:00:00Z").unwrap()
        );
        let row = fetch_by_device_code(&pool, "device-1").unwrap().unwrap();
        assert_eq!(row.account_id.as_deref(), Some(account_id.as_str()));
        assert!(consume_approved(&pool, "device-1", &account_id).unwrap());
        assert!(!consume_approved(&pool, "device-1", &account_id).unwrap());
    }
}
