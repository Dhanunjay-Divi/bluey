#![allow(clippy::doc_lazy_continuation)]
//! Codex Stage 18: one-time link codes for browser→app handoff.
//!
//! The browser-authenticated user hits POST /auth/link/mint, which
//! creates fresh access+refresh tokens and stores them keyed by a
//! random 32-byte code (sha256-hashed at rest). The browser receives
//! the raw code back and redirects to bluey://link?code=<raw>. The
//! Bluey app then POSTs /auth/link/exchange to atomically consume the
//! row and receive the tokens.

use anyhow::Result;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::RngCore;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::db::DbPool;

const CODE_BYTES: usize = 32;
const CODE_VALIDITY_MINS: i64 = 5;

fn hash_code(raw: &str) -> String {
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    hex::encode(h.finalize())
}

fn random_code() -> String {
    let mut buf = [0u8; CODE_BYTES];
    rand::thread_rng().fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Mint a new link code for `account_id`, storing the supplied access
/// + refresh tokens for later exchange. Returns the raw code (caller
/// includes it in the bluey:// deep link).
pub fn mint(
    pool: &DbPool,
    account_id: &str,
    access_token: &str,
    refresh_token: &str,
) -> Result<String> {
    crate::db::run_blocking_db(|| {
        let raw = random_code();
        let hash = hash_code(&raw);
        let expires_at = (Utc::now() + Duration::minutes(CODE_VALIDITY_MINS)).to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "INSERT INTO auth_link_codes
                    (code_hash, account_id, access_token, refresh_token, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![hash, account_id, access_token, refresh_token, expires_at],
                )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "INSERT INTO auth_link_codes
                    (code_hash, account_id, access_token, refresh_token, expires_at)
                 VALUES ($1, $2, $3, $4, $5::timestamptz)",
                    &[
                        &hash,
                        &account_id,
                        &access_token,
                        &refresh_token,
                        &expires_at,
                    ],
                )?;
            }
        }
        Ok(raw)
    })
}

/// One-time atomic consume. Returns Some((account_id, access, refresh))
/// when the code is valid + unexpired + unconsumed; None otherwise.
/// Uses UPDATE...RETURNING so concurrent consumes only succeed once.
pub fn exchange(pool: &DbPool, raw: &str) -> Result<Option<(String, String, String)>> {
    crate::db::run_blocking_db(|| {
        let hash = hash_code(raw);
        let now = Utc::now().to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let row: Option<(String, String, String)> = conn
                    .query_row(
                        "UPDATE auth_link_codes
                        SET consumed_at = datetime('now')
                      WHERE code_hash = ?1
                        AND consumed_at IS NULL
                        AND expires_at > ?2
                      RETURNING account_id, access_token, refresh_token",
                        params![hash, now],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                Ok(row)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    "UPDATE auth_link_codes
                    SET consumed_at = now()
                  WHERE code_hash = $1
                    AND consumed_at IS NULL
                    AND expires_at > $2::timestamptz
                  RETURNING account_id, access_token, refresh_token",
                    &[&hash, &now],
                )?;
                row.map(|row| Ok((row.try_get(0)?, row.try_get(1)?, row.try_get(2)?)))
                    .transpose()
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-link-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool) -> String {
        crate::db::accounts::Account::create(pool, "link@example.com", "stub")
            .unwrap()
            .id
    }

    #[test]
    fn mint_then_exchange_roundtrip() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let code = mint(&pool, &id, "access-tok-1", "refresh-tok-1").unwrap();
        let result = exchange(&pool, &code).unwrap().unwrap();
        assert_eq!(result.0, id);
        assert_eq!(result.1, "access-tok-1");
        assert_eq!(result.2, "refresh-tok-1");
    }

    #[test]
    fn exchange_is_single_use() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let code = mint(&pool, &id, "a", "r").unwrap();
        exchange(&pool, &code).unwrap();
        assert!(exchange(&pool, &code).unwrap().is_none());
    }

    #[test]
    fn exchange_rejects_unknown_code() {
        let pool = temp_pool();
        assert!(exchange(&pool, "not-a-real-code").unwrap().is_none());
    }
}
