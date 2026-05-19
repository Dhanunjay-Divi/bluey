//! Refresh token storage. Tokens are stored sha256-hashed so a DB
//! leak does not expose live tokens.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::db::DbPool;

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
    let expires_at = (Utc::now() + Duration::seconds(crate::auth::jwt::REFRESH_TTL_SECS))
        .to_rfc3339();
    let conn = pool.get()?;
    conn.execute(
        "INSERT OR REPLACE INTO refresh_tokens (token_hash, account_id, device_label, expires_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![hash_token(token), account_id, device_label, expires_at],
    )?;
    Ok(())
}

/// Validate that a refresh token is in the DB, not revoked, not expired.
/// Returns the account_id if valid; updates last_used_at as a side effect.
pub fn validate_and_touch(pool: &DbPool, token: &str) -> Result<Option<String>> {
    let conn = pool.get()?;
    let now = Utc::now();
    let token_hash = hash_token(token);
    let result: Option<(String, Option<String>, String)> = conn
        .query_row(
            "SELECT account_id, revoked_at, expires_at
             FROM refresh_tokens WHERE token_hash = ?1",
            params![&token_hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let Some((account_id, revoked_at, expires_at)) = result else {
        return Ok(None);
    };
    if revoked_at.is_some() {
        return Ok(None);
    }
    let exp: DateTime<Utc> = expires_at.parse()?;
    if exp < now {
        return Ok(None);
    }
    // Update last_used_at.
    conn.execute(
        "UPDATE refresh_tokens SET last_used_at = ?1 WHERE token_hash = ?2",
        params![now.to_rfc3339(), &token_hash],
    )?;
    Ok(Some(account_id))
}

/// Revoke a single refresh token (logout).
pub fn revoke(pool: &DbPool, token: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE refresh_tokens SET revoked_at = ?1 WHERE token_hash = ?2",
        params![Utc::now().to_rfc3339(), hash_token(token)],
    )?;
    Ok(())
}

/// Revoke all refresh tokens for an account (used on password change /
/// security event).
pub fn revoke_all_for_account(pool: &DbPool, account_id: &str) -> Result<usize> {
    let conn = pool.get()?;
    let n = conn.execute(
        "UPDATE refresh_tokens SET revoked_at = ?1
         WHERE account_id = ?2 AND revoked_at IS NULL",
        params![Utc::now().to_rfc3339(), account_id],
    )?;
    Ok(n)
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
        assert_eq!(validate_and_touch(&pool, "tok-1").unwrap(), Some(account_id));
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
}
