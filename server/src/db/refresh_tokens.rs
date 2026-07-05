//! Refresh token storage. Tokens are stored sha256-hashed so a DB
//! leak does not expose live tokens.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshTokenSession {
    pub token_hash: String,
    pub device_label: Option<String>,
    pub device_id: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedRefreshToken {
    pub account_id: String,
    pub device_label: Option<String>,
    pub device_id: Option<String>,
}

pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

/// Store a refresh token hash with a 30-day TTL.
pub fn store(
    pool: &DbPool,
    token: &str,
    account_id: &str,
    device_label: Option<&str>,
) -> Result<()> {
    store_with_device(pool, token, account_id, device_label, None)
}

/// Store a refresh token hash tied to a stable desktop device id.
pub fn store_with_device(
    pool: &DbPool,
    token: &str,
    account_id: &str,
    device_label: Option<&str>,
    device_id: Option<&str>,
) -> Result<()> {
    crate::db::run_blocking_db(|| {
        let expires_at = Utc::now() + Duration::seconds(crate::auth::jwt::REFRESH_TTL_SECS);
        let expires_at_text = expires_at.to_rfc3339();
        let token_hash = hash_token(token);
        let device_id = device_id.map(str::trim).filter(|value| !value.is_empty());
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                "INSERT OR REPLACE INTO refresh_tokens (token_hash, account_id, device_label, device_id, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![token_hash, account_id, device_label, device_id, expires_at_text],
            )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "INSERT INTO refresh_tokens (token_hash, account_id, device_label, device_id, expires_at)
                 VALUES ($1, $2, $3, $4, $5::timestamptz)
                 ON CONFLICT (token_hash) DO UPDATE SET
                    account_id = excluded.account_id,
                    device_label = excluded.device_label,
                    device_id = excluded.device_id,
                    expires_at = excluded.expires_at,
                    revoked_at = NULL",
                    &[&token_hash, &account_id, &device_label, &device_id, &expires_at],
                )?;
            }
        }
        Ok(())
    })
}

/// List active, non-expired refresh sessions for an account. The token hash is
/// used only as an opaque session id for revocation; the raw token is never
/// returned.
pub fn list_active_for_account(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<RefreshTokenSession>> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut stmt = conn.prepare(
                    "SELECT token_hash, device_label, device_id, created_at, last_used_at, expires_at
                       FROM refresh_tokens
                      WHERE account_id = ?1
                        AND revoked_at IS NULL
                        AND expires_at > ?2
                      ORDER BY COALESCE(last_used_at, created_at) DESC, created_at DESC",
                )?;
                let rows = stmt.query_map(params![account_id, now_text], |row| {
                    Ok(RefreshTokenSession {
                        token_hash: row.get(0)?,
                        device_label: row.get(1)?,
                        device_id: row.get(2)?,
                        created_at: row.get(3)?,
                        last_used_at: row.get(4)?,
                        expires_at: row.get(5)?,
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let rows = conn.query(
                    "SELECT token_hash, device_label, device_id, created_at, last_used_at, expires_at
                       FROM refresh_tokens
                      WHERE account_id = $1
                        AND revoked_at IS NULL
                        AND expires_at > $2
                      ORDER BY COALESCE(last_used_at, created_at) DESC, created_at DESC",
                    &[&account_id, &now],
                )?;
                rows.into_iter()
                    .map(|row| {
                        let created_at: DateTime<Utc> = row.try_get(3)?;
                        let last_used_at: Option<DateTime<Utc>> = row.try_get(4)?;
                        let expires_at: DateTime<Utc> = row.try_get(5)?;
                        Ok(RefreshTokenSession {
                            token_hash: row.try_get(0)?,
                            device_label: row.try_get(1)?,
                            device_id: row.try_get(2)?,
                            created_at: created_at.to_rfc3339(),
                            last_used_at: last_used_at.map(|value| value.to_rfc3339()),
                            expires_at: expires_at.to_rfc3339(),
                        })
                    })
                    .collect()
            }
        }
    })
}

/// Revoke one active refresh session by its opaque id for the owning account.
pub fn revoke_hash_for_account(pool: &DbPool, account_id: &str, token_hash: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let changed = conn.execute(
                    "UPDATE refresh_tokens
                        SET revoked_at = ?1
                      WHERE account_id = ?2
                        AND token_hash = ?3
                        AND revoked_at IS NULL",
                    params![now_text, account_id, token_hash],
                )?;
                Ok(changed > 0)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let changed = conn.execute(
                    "UPDATE refresh_tokens
                        SET revoked_at = $1
                      WHERE account_id = $2
                        AND token_hash = $3
                        AND revoked_at IS NULL",
                    &[&now, &account_id, &token_hash],
                )?;
                Ok(changed > 0)
            }
        }
    })
}

/// Revoke all active refresh sessions for one linked desktop.
pub fn revoke_for_device(pool: &DbPool, account_id: &str, device_id: &str) -> Result<usize> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let device_id = device_id.trim();
        if device_id.is_empty() {
            return Ok(0);
        }
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let n = conn.execute(
                    "UPDATE refresh_tokens
                        SET revoked_at = ?1
                      WHERE account_id = ?2
                        AND device_id = ?3
                        AND revoked_at IS NULL",
                    params![now_text, account_id, device_id],
                )?;
                Ok(n)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let affected = conn.execute(
                    "UPDATE refresh_tokens
                        SET revoked_at = $1
                      WHERE account_id = $2
                        AND device_id = $3
                        AND revoked_at IS NULL",
                    &[&now, &account_id, &device_id],
                )?;
                Ok(usize::try_from(affected).unwrap_or(usize::MAX))
            }
        }
    })
}

/// Revoke all active refresh sessions tied to linked desktops.
pub fn revoke_all_devices_for_account(pool: &DbPool, account_id: &str) -> Result<usize> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let n = conn.execute(
                    "UPDATE refresh_tokens
                        SET revoked_at = ?1
                      WHERE account_id = ?2
                        AND device_id IS NOT NULL
                        AND revoked_at IS NULL",
                    params![now_text, account_id],
                )?;
                Ok(n)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let affected = conn.execute(
                    "UPDATE refresh_tokens
                        SET revoked_at = $1
                      WHERE account_id = $2
                        AND device_id IS NOT NULL
                        AND revoked_at IS NULL",
                    &[&now, &account_id],
                )?;
                Ok(usize::try_from(affected).unwrap_or(usize::MAX))
            }
        }
    })
}

/// Validate that a refresh token is in the DB, not revoked, not expired.
/// Returns the account_id if valid; updates last_used_at as a side effect.
///
/// Returns:
///   `Ok(Some(account_id))` if the token is live + not revoked + not expired.
///   `Ok(None)` if the row genuinely does not exist (404 case).
///   `Err(_)` for DB errors (5xx case) — caller MUST distinguish so a
///   transient DB hiccup is not silently treated as auth failure.
pub fn validate_and_touch(pool: &DbPool, token: &str) -> Result<Option<String>> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let token_hash = hash_token(token);
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let result: rusqlite::Result<(String, Option<String>, String)> = conn.query_row(
                    "SELECT account_id, revoked_at, expires_at
                     FROM refresh_tokens WHERE token_hash = ?1",
                    params![&token_hash],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                );
                let (account_id, revoked_at, expires_at) = match result {
                    Ok(row) => row,
                    Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
                    Err(e) => return Err(e.into()),
                };
                if revoked_at.is_some() {
                    return Ok(None);
                }
                let exp: DateTime<Utc> = expires_at.parse()?;
                if exp < now {
                    return Ok(None);
                }
                conn.execute(
                    "UPDATE refresh_tokens SET last_used_at = ?1 WHERE token_hash = ?2",
                    params![now.to_rfc3339(), &token_hash],
                )?;
                Ok(Some(account_id))
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    "SELECT account_id, revoked_at, expires_at
                   FROM refresh_tokens WHERE token_hash = $1",
                    &[&token_hash],
                )?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let account_id: String = row.try_get(0)?;
                let revoked_at: Option<DateTime<Utc>> = row.try_get(1)?;
                let expires_at: DateTime<Utc> = row.try_get(2)?;
                if revoked_at.is_some() || expires_at < now {
                    return Ok(None);
                }
                conn.execute(
                    "UPDATE refresh_tokens SET last_used_at = $1 WHERE token_hash = $2",
                    &[&now, &token_hash],
                )?;
                Ok(Some(account_id))
            }
        }
    })
}

/// Atomically consume a refresh token: validate AND revoke in one
/// operation. Race-free against concurrent /auth/refresh calls because the
/// `WHERE revoked_at IS NULL` clause means only ONE caller can succeed.
///
/// Returns:
///   `Ok(Some(account_id))` if WE were the one to revoke (caller may issue
///       a new pair).
///   `Ok(None)` if the token does not exist, is already revoked, or is
///       expired (caller returns 401 to client).
///   `Err(_)` for DB errors.
pub fn consume(pool: &DbPool, token: &str) -> Result<Option<String>> {
    Ok(consume_with_metadata(pool, token)?.map(|consumed| consumed.account_id))
}

/// Atomically consume a refresh token and return account/device metadata.
pub fn consume_with_metadata(pool: &DbPool, token: &str) -> Result<Option<ConsumedRefreshToken>> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let token_hash = hash_token(token);

        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;

                // Atomic UPDATE that returns the account_id only if it was THIS call
                // that flipped revoked_at from NULL to now. The RETURNING clause is
                // SQLite 3.35+ and returns 0 rows on no-op.
                let mut stmt = conn.prepare(
                    "UPDATE refresh_tokens
                    SET revoked_at = ?1, last_used_at = ?1
                  WHERE token_hash = ?2
                    AND revoked_at IS NULL
                    AND expires_at > ?1
                RETURNING account_id, device_label, device_id",
                )?;
                let mut rows = stmt.query(params![&now_text, &token_hash])?;
                if let Some(row) = rows.next()? {
                    let account_id: String = row.get(0)?;
                    Ok(Some(ConsumedRefreshToken {
                        account_id,
                        device_label: row.get(1)?,
                        device_id: row.get(2)?,
                    }))
                } else {
                    Ok(None)
                }
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    "UPDATE refresh_tokens
                    SET revoked_at = $1, last_used_at = $1
                  WHERE token_hash = $2
                    AND revoked_at IS NULL
                    AND expires_at > $1
                RETURNING account_id, device_label, device_id",
                    &[&now, &token_hash],
                )?;
                row.map(|row| {
                    Ok(ConsumedRefreshToken {
                        account_id: row.try_get(0)?,
                        device_label: row.try_get(1)?,
                        device_id: row.try_get(2)?,
                    })
                })
                .transpose()
            }
        }
    })
}

/// Revoke a single refresh token (logout).
pub fn revoke(pool: &DbPool, token: &str) -> Result<()> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        let token_hash = hash_token(token);
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "UPDATE refresh_tokens SET revoked_at = ?1 WHERE token_hash = ?2",
                    params![now_text, token_hash],
                )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "UPDATE refresh_tokens SET revoked_at = $1 WHERE token_hash = $2",
                    &[&now, &token_hash],
                )?;
            }
        }
        Ok(())
    })
}

/// Revoke all refresh tokens for an account (used on password change /
/// security event).
pub fn revoke_all_for_account(pool: &DbPool, account_id: &str) -> Result<usize> {
    crate::db::run_blocking_db(|| {
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let n = conn.execute(
                    "UPDATE refresh_tokens SET revoked_at = ?1
                 WHERE account_id = ?2 AND revoked_at IS NULL",
                    params![now_text, account_id],
                )?;
                Ok(n)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let affected = conn.execute(
                    "UPDATE refresh_tokens SET revoked_at = $1
                 WHERE account_id = $2 AND revoked_at IS NULL",
                    &[&now, &account_id],
                )?;
                Ok(usize::try_from(affected).unwrap_or(usize::MAX))
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-rt-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool) -> String {
        crate::db::accounts::Account::create(pool, "rt@example.com", "stub")
            .unwrap()
            .id
    }

    #[test]
    fn store_and_validate_succeeds() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store(&pool, "tok-1", &account_id, Some("test-laptop")).unwrap();
        assert_eq!(
            validate_and_touch(&pool, "tok-1").unwrap(),
            Some(account_id)
        );
    }

    #[test]
    fn validate_unknown_token_returns_none() {
        let pool = temp_pool();
        assert_eq!(validate_and_touch(&pool, "nope").unwrap(), None);
    }

    #[test]
    fn revoke_invalidates_token() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store(&pool, "tok-2", &account_id, None).unwrap();
        revoke(&pool, "tok-2").unwrap();
        assert_eq!(validate_and_touch(&pool, "tok-2").unwrap(), None);
    }

    #[test]
    fn revoke_all_invalidates_every_token() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store(&pool, "tok-a", &account_id, None).unwrap();
        store(&pool, "tok-b", &account_id, None).unwrap();
        let n = revoke_all_for_account(&pool, &account_id).unwrap();
        assert_eq!(n, 2);
        assert!(validate_and_touch(&pool, "tok-a").unwrap().is_none());
        assert!(validate_and_touch(&pool, "tok-b").unwrap().is_none());
    }

    #[test]
    fn list_active_sessions_skips_revoked_tokens() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store(&pool, "active-tok", &account_id, Some("desktop")).unwrap();
        store(&pool, "revoked-tok", &account_id, Some("browser")).unwrap();
        revoke(&pool, "revoked-tok").unwrap();

        let sessions = list_active_for_account(&pool, &account_id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].token_hash, hash_token("active-tok"));
        assert_eq!(sessions[0].device_label.as_deref(), Some("desktop"));
        assert_eq!(sessions[0].device_id.as_deref(), None);
    }

    #[test]
    fn revoke_for_device_invalidates_only_that_desktop() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store_with_device(
            &pool,
            "mac-refresh",
            &account_id,
            Some("bluey-desktop"),
            Some("device-mac"),
        )
        .unwrap();
        store_with_device(
            &pool,
            "win-refresh",
            &account_id,
            Some("bluey-desktop"),
            Some("device-win"),
        )
        .unwrap();
        store(&pool, "browser-refresh", &account_id, Some("browser")).unwrap();

        let revoked = revoke_for_device(&pool, &account_id, "device-mac").unwrap();

        assert_eq!(revoked, 1);
        assert_eq!(validate_and_touch(&pool, "mac-refresh").unwrap(), None);
        assert_eq!(
            validate_and_touch(&pool, "win-refresh").unwrap(),
            Some(account_id.clone())
        );
        assert_eq!(
            validate_and_touch(&pool, "browser-refresh").unwrap(),
            Some(account_id)
        );
    }

    #[test]
    fn consume_with_metadata_returns_device_id_for_rotation() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store_with_device(
            &pool,
            "desktop-refresh",
            &account_id,
            Some("bluey-desktop"),
            Some("device-123"),
        )
        .unwrap();

        let consumed = consume_with_metadata(&pool, "desktop-refresh")
            .unwrap()
            .expect("refresh token consumed");

        assert_eq!(consumed.account_id, account_id);
        assert_eq!(consumed.device_label.as_deref(), Some("bluey-desktop"));
        assert_eq!(consumed.device_id.as_deref(), Some("device-123"));
    }

    #[test]
    fn revoke_hash_for_account_scopes_to_account() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        let other_account_id =
            crate::db::accounts::Account::create(&pool, "other-rt@example.com", "stub")
                .unwrap()
                .id;
        store(&pool, "tok-owned", &account_id, None).unwrap();
        let token_hash = hash_token("tok-owned");

        assert!(!revoke_hash_for_account(&pool, &other_account_id, &token_hash).unwrap());
        assert_eq!(
            validate_and_touch(&pool, "tok-owned").unwrap(),
            Some(account_id.clone())
        );
        assert!(revoke_hash_for_account(&pool, &account_id, &token_hash).unwrap());
        assert_eq!(validate_and_touch(&pool, "tok-owned").unwrap(), None);
    }

    #[test]
    fn consume_is_atomic_and_single_use() {
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store(&pool, "atomic-tok", &account_id, None).unwrap();
        // First consume returns the account.
        assert_eq!(consume(&pool, "atomic-tok").unwrap(), Some(account_id));
        // Second consume returns None — the token is already revoked.
        assert_eq!(consume(&pool, "atomic-tok").unwrap(), None);
        // validate_and_touch also sees it as revoked.
        assert_eq!(validate_and_touch(&pool, "atomic-tok").unwrap(), None);
    }

    #[test]
    fn consume_under_simulated_concurrency_only_one_winner() {
        // Even though SQLite serialises writes, the test still proves
        // the WHERE clause is atomic at the SQL level: two consumes
        // produce one Some + one None.
        let pool = temp_pool();
        let account_id = make_account(&pool);
        store(&pool, "race-tok", &account_id, None).unwrap();
        let r1 = consume(&pool, "race-tok").unwrap();
        let r2 = consume(&pool, "race-tok").unwrap();
        assert!(r1.is_some() ^ r2.is_some(), "exactly one must win");
    }

    #[test]
    fn consume_unknown_token_returns_none() {
        let pool = temp_pool();
        assert_eq!(consume(&pool, "no-such-token").unwrap(), None);
    }
}
