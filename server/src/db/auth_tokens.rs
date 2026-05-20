//! Email verification + password reset tokens. Codex Stage 13.
//!
//! Single-use, 24h expiry, sha256-hashed at rest. `start` handlers
//! deliver links through SMTP when configured; otherwise they log a
//! dev-only URL when `BLUEY_SMTP_HOST` is unset.

use anyhow::Result;
use base64::Engine;
use chrono::{Duration, Utc};
use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::db::DbPool;

const TOKEN_BYTES: usize = 32;
const TOKEN_VALIDITY_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy)]
pub enum TokenKind {
    EmailVerification,
    PasswordReset,
}

impl TokenKind {
    fn table(&self) -> &'static str {
        match self {
            Self::EmailVerification => "email_verification_tokens",
            Self::PasswordReset => "password_reset_tokens",
        }
    }
}

fn hash_token(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    hex::encode(h.finalize())
}

fn random_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Mint a fresh token for `account_id` of `kind`. Returns the raw token
/// (caller emails it to the customer).
pub fn mint(pool: &DbPool, account_id: &str, kind: TokenKind) -> Result<String> {
    let raw = random_token();
    let hash = hash_token(&raw);
    let expires_at = (Utc::now() + Duration::hours(TOKEN_VALIDITY_HOURS)).to_rfc3339();
    let conn = pool.get()?;
    conn.execute(
        &format!(
            "INSERT INTO {} (token_hash, account_id, expires_at) VALUES (?1, ?2, ?3)",
            kind.table()
        ),
        params![hash, account_id, expires_at],
    )?;
    Ok(raw)
}

/// Atomically consume a token. Returns the account_id if the token is
/// valid + unexpired + unconsumed; None otherwise. Marks consumed_at
/// in the same UPDATE so a concurrent consume only succeeds once.
pub fn consume(pool: &DbPool, raw: &str, kind: TokenKind) -> Result<Option<String>> {
    let hash = hash_token(raw);
    let now = Utc::now().to_rfc3339();
    let conn = pool.get()?;
    let row: Option<String> = conn
        .query_row(
            &format!(
                "UPDATE {}
                    SET consumed_at = datetime('now')
                  WHERE token_hash = ?1
                    AND consumed_at IS NULL
                    AND expires_at > ?2
                  RETURNING account_id",
                kind.table()
            ),
            params![hash, now],
            |r| r.get::<_, String>(0),
        )
        .ok();
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-tok-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool) -> String {
        crate::db::accounts::Account::create(pool, "tok@example.com", "stub")
            .unwrap()
            .id
    }

    #[test]
    fn mint_then_consume_email_verification() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let tok = mint(&pool, &id, TokenKind::EmailVerification).unwrap();
        assert_eq!(
            consume(&pool, &tok, TokenKind::EmailVerification).unwrap(),
            Some(id)
        );
    }

    #[test]
    fn consume_is_single_use() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let tok = mint(&pool, &id, TokenKind::PasswordReset).unwrap();
        consume(&pool, &tok, TokenKind::PasswordReset).unwrap();
        assert_eq!(
            consume(&pool, &tok, TokenKind::PasswordReset).unwrap(),
            None
        );
    }

    #[test]
    fn token_kinds_are_isolated() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let tok = mint(&pool, &id, TokenKind::EmailVerification).unwrap();
        // Same raw token must NOT consume from the password_reset table.
        assert_eq!(
            consume(&pool, &tok, TokenKind::PasswordReset).unwrap(),
            None
        );
    }
}
