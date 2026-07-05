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
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceCodeMetadata {
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub app_version: Option<String>,
}

pub fn insert(
    pool: &DbPool,
    device_code: &str,
    user_code: &str,
    expires_at: &str,
    metadata: Option<&DeviceCodeMetadata>,
) -> Result<()> {
    crate::db::run_blocking_db(|| {
        let metadata = normalize_metadata(metadata.cloned().unwrap_or_default());
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                if let Some(device_id) = metadata.device_id.as_deref() {
                    conn.execute(
                        "DELETE FROM device_codes WHERE device_id = ?1",
                        params![device_id],
                    )?;
                }
                conn.execute(
                    "INSERT INTO device_codes (
                        device_code, user_code, expires_at,
                        device_id, device_name, platform, arch, app_version
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        device_code,
                        user_code,
                        expires_at,
                        metadata.device_id,
                        metadata.device_name,
                        metadata.platform,
                        metadata.arch,
                        metadata.app_version,
                    ],
                )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let expires_at =
                    chrono::DateTime::parse_from_rfc3339(expires_at)?.with_timezone(&chrono::Utc);
                if let Some(device_id) = metadata.device_id.as_deref() {
                    conn.execute(
                        "DELETE FROM device_codes WHERE device_id = $1",
                        &[&device_id],
                    )?;
                }
                conn.execute(
                    "INSERT INTO device_codes (
                        device_code, user_code, expires_at,
                        device_id, device_name, platform, arch, app_version
                     )
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &device_code,
                        &user_code,
                        &expires_at,
                        &metadata.device_id,
                        &metadata.device_name,
                        &metadata.platform,
                        &metadata.arch,
                        &metadata.app_version,
                    ],
                )?;
            }
        }
        Ok(())
    })
}

fn normalize_metadata(metadata: DeviceCodeMetadata) -> DeviceCodeMetadata {
    DeviceCodeMetadata {
        device_id: clean(metadata.device_id),
        device_name: clean(metadata.device_name),
        platform: clean(metadata.platform),
        arch: clean(metadata.arch),
        app_version: clean(metadata.app_version),
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().chars().take(128).collect::<String>())
        .filter(|value| !value.is_empty())
}

pub fn fetch_by_device_code(pool: &DbPool, device_code: &str) -> Result<Option<DeviceCodeRow>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let row = conn
                .query_row(
                    "SELECT account_id, approved, expires_at, device_id, device_name, platform, arch, app_version
                       FROM device_codes
                      WHERE device_code = ?1",
                    params![device_code],
                    |r| {
                        Ok(DeviceCodeRow {
                            account_id: r.get(0)?,
                            approved: r.get(1)?,
                            expires_at: r.get(2)?,
                            device_id: r.get(3)?,
                            device_name: r.get(4)?,
                            platform: r.get(5)?,
                            arch: r.get(6)?,
                            app_version: r.get(7)?,
                        })
                    },
                )
                .ok();
            Ok(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT account_id, approved, expires_at, device_id, device_name, platform, arch, app_version
                   FROM device_codes
                  WHERE device_code = $1",
                &[&device_code],
            )?;
            row.map(|row| {
                let expires_at: chrono::DateTime<chrono::Utc> = row.try_get(2)?;
                Ok(DeviceCodeRow {
                    account_id: row.try_get(0)?,
                    approved: row.try_get::<_, i32>(1)? as i64,
                    expires_at: expires_at.to_rfc3339(),
                    device_id: row.try_get(3)?,
                    device_name: row.try_get(4)?,
                    platform: row.try_get(5)?,
                    arch: row.try_get(6)?,
                    app_version: row.try_get(7)?,
                })
            })
            .transpose()
        }
    })
}

pub fn consume_approved(pool: &DbPool, device_code: &str, account_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
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
    })
}

pub fn approve_user_code(
    pool: &DbPool,
    account_id: &str,
    user_code: &str,
    now: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
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
            let now = chrono::DateTime::parse_from_rfc3339(now)?.with_timezone(&chrono::Utc);
            let updated = conn.execute(
                "UPDATE device_codes SET approved = 1, account_id = $1
                 WHERE user_code = $2 AND expires_at > $3 AND approved = 0",
                &[&account_id, &user_code, &now],
            )?;
            Ok(updated > 0)
        }
    })
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
        let metadata = DeviceCodeMetadata {
            device_id: Some("stable-device".to_string()),
            device_name: Some("Uno Mac".to_string()),
            platform: Some("macos".to_string()),
            arch: Some("aarch64".to_string()),
            app_version: Some("0.1.86".to_string()),
        };
        insert(
            &pool,
            "device-1",
            "USER-123",
            "2099-01-01T00:00:00Z",
            Some(&metadata),
        )
        .unwrap();
        assert!(approve_user_code(&pool, &account_id, "USER-123", "2026-01-01T00:00:00Z").unwrap());
        let row = fetch_by_device_code(&pool, "device-1").unwrap().unwrap();
        assert_eq!(row.account_id.as_deref(), Some(account_id.as_str()));
        assert_eq!(row.device_id.as_deref(), Some("stable-device"));
        assert_eq!(row.device_name.as_deref(), Some("Uno Mac"));
        assert!(consume_approved(&pool, "device-1", &account_id).unwrap());
        assert!(!consume_approved(&pool, "device-1", &account_id).unwrap());
    }

    #[test]
    fn new_code_for_same_device_invalidates_old_code() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        let metadata = DeviceCodeMetadata {
            device_id: Some("stable-device".to_string()),
            device_name: Some("Uno Mac".to_string()),
            platform: Some("macos".to_string()),
            arch: None,
            app_version: None,
        };
        insert(
            &pool,
            "device-old",
            "OLD-1234",
            "2099-01-01T00:00:00Z",
            Some(&metadata),
        )
        .unwrap();
        insert(
            &pool,
            "device-new",
            "NEW-1234",
            "2099-01-01T00:00:00Z",
            Some(&metadata),
        )
        .unwrap();

        assert!(fetch_by_device_code(&pool, "device-old").unwrap().is_none());
        assert!(
            !approve_user_code(&pool, &account_id, "OLD-1234", "2026-01-01T00:00:00Z").unwrap()
        );
        assert!(approve_user_code(&pool, &account_id, "NEW-1234", "2026-01-01T00:00:00Z").unwrap());
    }
}
