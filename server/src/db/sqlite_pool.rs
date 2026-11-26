//! Broken-aware custody for the existing SQLite pool manager.

use std::ops::{Deref, DerefMut};

use r2d2::ManageConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

/// Wraps the configured manager without changing its connection or init policy.
pub struct SafeSqliteConnectionManager {
    inner: SqliteConnectionManager,
}

/// A connection that its owner can explicitly invalidate before pool check-in.
pub struct SafeSqliteClient {
    inner: Connection,
    broken: bool,
}

impl SafeSqliteConnectionManager {
    pub(crate) fn new(inner: SqliteConnectionManager) -> Self {
        Self { inner }
    }
}

impl From<SqliteConnectionManager> for SafeSqliteConnectionManager {
    fn from(inner: SqliteConnectionManager) -> Self {
        Self::new(inner)
    }
}

impl SafeSqliteClient {
    pub(crate) fn mark_broken(&mut self) {
        self.broken = true;
    }
}

impl Deref for SafeSqliteClient {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SafeSqliteClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ManageConnection for SafeSqliteConnectionManager {
    type Connection = SafeSqliteClient;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        self.inner.connect().map(|inner| SafeSqliteClient {
            inner,
            broken: false,
        })
    }

    fn is_valid(&self, client: &mut Self::Connection) -> Result<(), Self::Error> {
        if client.broken {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.inner.is_valid(&mut client.inner)
    }

    fn has_broken(&self, client: &mut Self::Connection) -> bool {
        client.broken || self.inner.has_broken(&mut client.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2::Pool;

    fn manager() -> SafeSqliteConnectionManager {
        SafeSqliteConnectionManager::new(
            SqliteConnectionManager::memory()
                .with_init(|conn| conn.execute_batch("PRAGMA foreign_keys = ON;")),
        )
    }

    #[test]
    fn healthy_checkout_reuses_connection_and_preserves_temporary_state() {
        let pool = Pool::builder().max_size(1).build(manager()).unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute_batch("CREATE TEMP TABLE phase657_pool_marker(value INTEGER);")
                .unwrap();
        }
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_temp_master WHERE name='phase657_pool_marker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn explicitly_marked_client_is_invalid_and_broken() {
        let manager = manager();
        let mut client = manager.connect().unwrap();
        assert!(manager.is_valid(&mut client).is_ok());
        assert!(!manager.has_broken(&mut client));
        client.mark_broken();
        assert!(manager.is_valid(&mut client).is_err());
        assert!(manager.has_broken(&mut client));
    }

    #[test]
    fn marked_checkout_with_open_transaction_and_foreign_keys_off_is_discarded() {
        let pool = Pool::builder().max_size(1).build(manager()).unwrap();
        {
            let mut conn = pool.get().unwrap();
            conn.execute_batch(
                "CREATE TABLE phase657_shared_marker(value INTEGER);
                 INSERT INTO phase657_shared_marker VALUES (7);
                 CREATE TEMP TABLE phase657_pool_marker(value INTEGER);
                 PRAGMA foreign_keys = OFF;
                 BEGIN IMMEDIATE;",
            )
            .unwrap();
            let foreign_keys: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
                .unwrap();
            assert_eq!(foreign_keys, 0);
            assert!(!conn.is_autocommit());
            conn.mark_broken();
        }
        let conn = pool.get().unwrap();
        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        let marker: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_temp_master WHERE name='phase657_pool_marker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 1);
        assert!(conn.is_autocommit());
        assert_eq!(marker, 0);
        let shared_value: i64 = conn
            .query_row("SELECT value FROM phase657_shared_marker", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(shared_value, 7);
    }

    #[test]
    fn migration_list_error_discards_its_uncertain_checkout() {
        let pool = Pool::builder().max_size(1).build(manager()).unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute_batch(
                "CREATE TEMP TABLE phase657_pool_marker(value INTEGER);
                 PRAGMA foreign_keys = OFF;
                 BEGIN IMMEDIATE;",
            )
            .unwrap();
            assert!(!conn.is_autocommit());
        }
        let pool = super::super::DbPool::Sqlite(pool);
        let error = super::super::run_sqlite_migration_list(&pool, &[]).unwrap_err();
        assert!(error
            .to_string()
            .contains("SQLite signed job-integrity migration is missing"));
        let conn = pool.get().unwrap();
        assert!(conn.is_autocommit());
        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        let marker: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_temp_master WHERE name='phase657_pool_marker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(marker, 0);
    }
}
