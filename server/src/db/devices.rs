//! Stable desktop device records for the account dashboard.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    pub id: String,
    pub account_id: String,
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub arch: Option<String>,
    pub app_version: Option<String>,
    pub registered_at: String,
    pub last_seen_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRegistration {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
    pub arch: Option<String>,
    pub app_version: Option<String>,
}

pub fn upsert(
    pool: &DbPool,
    account_id: &str,
    registration: &DeviceRegistration,
) -> Result<DeviceRecord> {
    crate::db::run_blocking_db(|| {
        let id = format!("devrow_{}", uuid::Uuid::new_v4().simple());
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let device_id = clean(&registration.device_id);
        let device_name = clean(&registration.device_name);
        let platform = clean(&registration.platform);
        let arch = registration.arch.as_deref().map(clean);
        let app_version = registration.app_version.as_deref().map(clean);
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "UPDATE account_devices
                        SET revoked_at = ?1
                      WHERE device_id = ?2
                        AND account_id <> ?3
                        AND revoked_at IS NULL",
                    params![now_text, device_id, account_id],
                )?;
                conn.execute(
                    "INSERT INTO account_devices (
                        id, account_id, device_id, device_name, platform, arch, app_version,
                        registered_at, last_seen_at, last_heartbeat_at, revoked_at
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, NULL)
                     ON CONFLICT(account_id, device_id) DO UPDATE SET
                        device_name = excluded.device_name,
                        platform = excluded.platform,
                        arch = excluded.arch,
                        app_version = excluded.app_version,
                        last_seen_at = excluded.last_seen_at,
                        last_heartbeat_at = excluded.last_heartbeat_at,
                        revoked_at = NULL",
                    params![
                        id,
                        account_id,
                        device_id,
                        device_name,
                        platform,
                        arch,
                        app_version,
                        now_text,
                    ],
                )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "UPDATE account_devices
                        SET revoked_at = $1
                      WHERE device_id = $2
                        AND account_id <> $3
                        AND revoked_at IS NULL",
                    &[&now, &device_id, &account_id],
                )?;
                conn.execute(
                    "INSERT INTO account_devices (
                        id, account_id, device_id, device_name, platform, arch, app_version,
                        registered_at, last_seen_at, last_heartbeat_at, revoked_at
                     )
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $8, NULL)
                     ON CONFLICT(account_id, device_id) DO UPDATE SET
                        device_name = excluded.device_name,
                        platform = excluded.platform,
                        arch = excluded.arch,
                        app_version = excluded.app_version,
                        last_seen_at = excluded.last_seen_at,
                        last_heartbeat_at = excluded.last_heartbeat_at,
                        revoked_at = NULL",
                    &[
                        &id,
                        &account_id,
                        &device_id,
                        &device_name,
                        &platform,
                        &arch,
                        &app_version,
                        &now,
                    ],
                )?;
            }
        }
        fetch_by_stable_id(pool, account_id, &registration.device_id)?
            .ok_or_else(|| anyhow::anyhow!("upserted device not found"))
    })
}

pub fn list_for_account(pool: &DbPool, account_id: &str) -> Result<Vec<DeviceRecord>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, account_id, device_id, device_name, platform, arch, app_version,
                        registered_at, last_seen_at, last_heartbeat_at
                   FROM account_devices
                  WHERE account_id = ?1
                    AND revoked_at IS NULL
                  ORDER BY COALESCE(last_heartbeat_at, last_seen_at, registered_at) DESC",
            )?;
            let rows = stmt.query_map(params![account_id], sqlite_record)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(Into::into)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let rows = conn.query(
                "SELECT id, account_id, device_id, device_name, platform, arch, app_version,
                        registered_at, last_seen_at, last_heartbeat_at
                   FROM account_devices
                  WHERE account_id = $1
                    AND revoked_at IS NULL
                  ORDER BY COALESCE(last_heartbeat_at, last_seen_at, registered_at) DESC",
                &[&account_id],
            )?;
            rows.into_iter().map(pg_record).collect()
        }
    })
}

pub fn revoke_for_account(pool: &DbPool, account_id: &str, id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let changed = conn.execute(
                    "UPDATE account_devices
                        SET revoked_at = ?1
                      WHERE account_id = ?2
                        AND id = ?3
                        AND revoked_at IS NULL",
                    params![now_text, account_id, id],
                )?;
                Ok(changed > 0)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let changed = conn.execute(
                    "UPDATE account_devices
                        SET revoked_at = $1
                      WHERE account_id = $2
                        AND id = $3
                        AND revoked_at IS NULL",
                    &[&now, &account_id, &id],
                )?;
                Ok(changed > 0)
            }
        }
    })
}

pub fn is_active_for_account(pool: &DbPool, account_id: &str, device_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        let device_id = clean(device_id);
        if device_id.is_empty() {
            return Ok(false);
        }
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let exists: i64 = conn.query_row(
                    "SELECT COUNT(*)
                       FROM account_devices
                      WHERE account_id = ?1
                        AND device_id = ?2
                        AND revoked_at IS NULL",
                    params![account_id, device_id],
                    |row| row.get(0),
                )?;
                Ok(exists > 0)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_one(
                    "SELECT COUNT(*)::bigint
                       FROM account_devices
                      WHERE account_id = $1
                        AND device_id = $2
                        AND revoked_at IS NULL",
                    &[&account_id, &device_id],
                )?;
                let exists: i64 = row.try_get(0)?;
                Ok(exists > 0)
            }
        }
    })
}

pub fn revoke_all_for_account(pool: &DbPool, account_id: &str) -> Result<usize> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "UPDATE account_devices
                        SET revoked_at = ?1
                      WHERE account_id = ?2
                        AND revoked_at IS NULL",
                    params![now_text, account_id],
                )
                .map_err(Into::into)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "UPDATE account_devices
                        SET revoked_at = $1
                      WHERE account_id = $2
                        AND revoked_at IS NULL",
                    &[&now, &account_id],
                )
                .map(|changed| changed as usize)
                .map_err(Into::into)
            }
        }
    })
}

fn fetch_by_stable_id(
    pool: &DbPool,
    account_id: &str,
    device_id: &str,
) -> Result<Option<DeviceRecord>> {
    let device_id = clean(device_id);
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let result = conn.query_row(
                "SELECT id, account_id, device_id, device_name, platform, arch, app_version,
                        registered_at, last_seen_at, last_heartbeat_at
                   FROM account_devices
                  WHERE account_id = ?1
                    AND device_id = ?2
                    AND revoked_at IS NULL",
                params![account_id, device_id],
                sqlite_record,
            );
            match result {
                Ok(record) => Ok(Some(record)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT id, account_id, device_id, device_name, platform, arch, app_version,
                        registered_at, last_seen_at, last_heartbeat_at
                   FROM account_devices
                  WHERE account_id = $1
                    AND device_id = $2
                    AND revoked_at IS NULL",
                &[&account_id, &device_id],
            )?;
            row.map(pg_record).transpose()
        }
    }
}

fn sqlite_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeviceRecord> {
    Ok(DeviceRecord {
        id: row.get(0)?,
        account_id: row.get(1)?,
        device_id: row.get(2)?,
        device_name: row.get(3)?,
        platform: row.get(4)?,
        arch: row.get(5)?,
        app_version: row.get(6)?,
        registered_at: row.get(7)?,
        last_seen_at: row.get(8)?,
        last_heartbeat_at: row.get(9)?,
    })
}

fn pg_record(row: postgres::Row) -> Result<DeviceRecord> {
    let registered_at: DateTime<Utc> = row.try_get(7)?;
    let last_seen_at: Option<DateTime<Utc>> = row.try_get(8)?;
    let last_heartbeat_at: Option<DateTime<Utc>> = row.try_get(9)?;
    Ok(DeviceRecord {
        id: row.try_get(0)?,
        account_id: row.try_get(1)?,
        device_id: row.try_get(2)?,
        device_name: row.try_get(3)?,
        platform: row.try_get(4)?,
        arch: row.try_get(5)?,
        app_version: row.try_get(6)?,
        registered_at: registered_at.to_rfc3339(),
        last_seen_at: last_seen_at.map(|value| value.to_rfc3339()),
        last_heartbeat_at: last_heartbeat_at.map(|value| value.to_rfc3339()),
    })
}

fn clean(value: &str) -> String {
    value.trim().chars().take(128).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-devices-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool, email: &str) -> String {
        let password_hash = crate::auth::password::hash_password("password123").unwrap();
        crate::db::accounts::Account::create(pool, email, &password_hash)
            .unwrap()
            .id
    }

    #[test]
    fn same_account_and_device_id_upserts() {
        let pool = temp_pool();
        let account_id = make_account(&pool, "device-upsert@example.com");
        let first = DeviceRegistration {
            device_id: "stable-device".to_string(),
            device_name: "Uno Mac".to_string(),
            platform: "macos".to_string(),
            arch: Some("aarch64".to_string()),
            app_version: Some("0.1.0".to_string()),
        };
        let second = DeviceRegistration {
            device_id: "stable-device".to_string(),
            device_name: "Renamed Mac".to_string(),
            platform: "macos".to_string(),
            arch: Some("aarch64".to_string()),
            app_version: Some("0.1.1".to_string()),
        };

        let first_record = upsert(&pool, &account_id, &first).unwrap();
        let second_record = upsert(&pool, &account_id, &second).unwrap();
        let devices = list_for_account(&pool, &account_id).unwrap();

        assert_eq!(first_record.id, second_record.id);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_name, "Renamed Mac");
        assert_eq!(devices[0].app_version.as_deref(), Some("0.1.1"));
    }

    #[test]
    fn same_device_id_moves_between_accounts() {
        let pool = temp_pool();
        let account_a = make_account(&pool, "device-a@example.com");
        let account_b = make_account(&pool, "device-b@example.com");
        let registration = DeviceRegistration {
            device_id: "stable-device".to_string(),
            device_name: "Uno Mac".to_string(),
            platform: "macos".to_string(),
            arch: None,
            app_version: None,
        };

        upsert(&pool, &account_a, &registration).unwrap();
        upsert(&pool, &account_b, &registration).unwrap();

        assert_eq!(list_for_account(&pool, &account_a).unwrap().len(), 0);
        assert_eq!(list_for_account(&pool, &account_b).unwrap().len(), 1);

        upsert(&pool, &account_a, &registration).unwrap();

        assert_eq!(list_for_account(&pool, &account_a).unwrap().len(), 1);
        assert_eq!(list_for_account(&pool, &account_b).unwrap().len(), 0);
    }

    #[test]
    fn is_active_for_account_tracks_revocation() {
        let pool = temp_pool();
        let account_id = make_account(&pool, "device-active@example.com");
        let registration = DeviceRegistration {
            device_id: "active-device".to_string(),
            device_name: "Active Mac".to_string(),
            platform: "macos".to_string(),
            arch: None,
            app_version: None,
        };

        let record = upsert(&pool, &account_id, &registration).unwrap();
        assert!(is_active_for_account(&pool, &account_id, "active-device").unwrap());

        revoke_for_account(&pool, &account_id, &record.id).unwrap();
        assert!(!is_active_for_account(&pool, &account_id, "active-device").unwrap());
    }
}
