//! Signup OTP storage. Kept in the DB layer so auth routes do not bind
//! directly to the SQLite runtime while we prepare the Postgres adapter.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignupOtp {
    pub otp_hash: String,
    pub password_hash: String,
    pub expires_at: String,
    pub attempts: i64,
}

pub fn upsert(
    pool: &DbPool,
    email: &str,
    otp_hash: &str,
    password_hash: &str,
    expires_at: &str,
) -> Result<()> {
    crate::db::run_blocking_db(|| {
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "INSERT INTO signup_otps (email, otp_hash, password_hash, attempts, expires_at)
                 VALUES (?1, ?2, ?3, 0, ?4)
                 ON CONFLICT(email) DO UPDATE SET
                    otp_hash = excluded.otp_hash,
                    password_hash = excluded.password_hash,
                    attempts = 0,
                    created_at = datetime('now'),
                    expires_at = excluded.expires_at",
                    params![email, otp_hash, password_hash, expires_at],
                )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "INSERT INTO signup_otps (email, otp_hash, password_hash, attempts, expires_at)
                 VALUES ($1, $2, $3, 0, $4::timestamptz)
                 ON CONFLICT(email) DO UPDATE SET
                    otp_hash = excluded.otp_hash,
                    password_hash = excluded.password_hash,
                    attempts = 0,
                    created_at = now(),
                    expires_at = excluded.expires_at",
                    &[&email, &otp_hash, &password_hash, &expires_at],
                )?;
            }
        }
        Ok(())
    })
}

pub fn fetch(pool: &DbPool, email: &str) -> Result<Option<SignupOtp>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let row = conn
                .query_row(
                    "SELECT otp_hash, password_hash, expires_at, attempts
                     FROM signup_otps WHERE email = ?1",
                    params![email],
                    |r| {
                        Ok(SignupOtp {
                            otp_hash: r.get(0)?,
                            password_hash: r.get(1)?,
                            expires_at: r.get(2)?,
                            attempts: r.get(3)?,
                        })
                    },
                )
                .ok();
            Ok(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT otp_hash, password_hash, expires_at, attempts
                 FROM signup_otps WHERE email = $1",
                &[&email],
            )?;
            row.map(|row| {
                let expires_at: chrono::DateTime<chrono::Utc> = row.try_get(2)?;
                Ok(SignupOtp {
                    otp_hash: row.try_get(0)?,
                    password_hash: row.try_get(1)?,
                    expires_at: expires_at.to_rfc3339(),
                    attempts: row.try_get(3)?,
                })
            })
            .transpose()
        }
    })
}

pub fn increment_attempts(pool: &DbPool, email: &str) -> Result<usize> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE signup_otps SET attempts = attempts + 1 WHERE email = ?1",
                params![email],
            )?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let affected = conn.execute(
                "UPDATE signup_otps SET attempts = attempts + 1 WHERE email = $1",
                &[&email],
            )?;
            Ok(usize::try_from(affected).unwrap_or(usize::MAX))
        }
    })
}

pub fn delete(pool: &DbPool, email: &str) -> Result<usize> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.execute("DELETE FROM signup_otps WHERE email = ?1", params![email])?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let affected = conn.execute("DELETE FROM signup_otps WHERE email = $1", &[&email])?;
            Ok(usize::try_from(affected).unwrap_or(usize::MAX))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-otp-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    #[test]
    fn upsert_resets_attempts_and_updates_hashes() {
        let pool = temp_pool();
        upsert(
            &pool,
            "a@example.com",
            "otp1",
            "pw1",
            "2099-01-01T00:00:00Z",
        )
        .unwrap();
        increment_attempts(&pool, "a@example.com").unwrap();
        upsert(
            &pool,
            "a@example.com",
            "otp2",
            "pw2",
            "2099-01-02T00:00:00Z",
        )
        .unwrap();

        let row = fetch(&pool, "a@example.com").unwrap().unwrap();
        assert_eq!(row.otp_hash, "otp2");
        assert_eq!(row.password_hash, "pw2");
        assert_eq!(row.attempts, 0);
    }
}
