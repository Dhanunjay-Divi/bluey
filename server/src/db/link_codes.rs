#![allow(clippy::doc_lazy_continuation)]
//! Codex Stage 18: one-time link codes for browser→app handoff.
//!
//! The browser-authenticated user hits POST /auth/link/mint, which
//! creates a short-lived authorization grant keyed by a random 32-byte
//! code (sha256-hashed at rest). The browser receives the raw code and
//! redirects to bluey://link?code=<raw>. The Bluey app then POSTs
//! /auth/link/exchange to atomically consume the grant. Credentials are
//! issued only after that exchange, so this table never needs to retain
//! live access or refresh tokens.

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

/// Mint a new link code for `account_id`. Returns the raw code for the
/// caller to place in the `bluey://` deep link.
///
/// The legacy token columns remain as empty compatibility fields until a
/// later schema migration can remove them from every supported database.
pub fn mint(pool: &DbPool, account_id: &str) -> Result<String> {
    crate::db::run_blocking_db(|| {
        let raw = random_code();
        let hash = hash_code(&raw);
        let expires_at = Utc::now() + Duration::minutes(CODE_VALIDITY_MINS);
        let expires_at_text = expires_at.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "INSERT INTO auth_link_codes
                    (code_hash, account_id, access_token, refresh_token, expires_at)
                 VALUES (?1, ?2, '', '', ?3)",
                    params![hash, account_id, expires_at_text],
                )?;
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "INSERT INTO auth_link_codes
                    (code_hash, account_id, access_token, refresh_token, expires_at)
                 VALUES ($1, $2, '', '', $3::timestamptz)",
                    &[&hash, &account_id, &expires_at],
                )?;
            }
        }
        Ok(raw)
    })
}

/// One-time atomic consume. Returns the authorized account id when the code
/// is valid, unexpired, and unconsumed. Legacy credential columns are scrubbed
/// in the same statement, including for grants minted by an older server.
pub fn exchange(pool: &DbPool, raw: &str) -> Result<Option<String>> {
    crate::db::run_blocking_db(|| {
        let hash = hash_code(raw);
        let now = Utc::now();
        let now_text = now.to_rfc3339();
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let row: Option<String> = conn
                    .query_row(
                        "UPDATE auth_link_codes
                        SET consumed_at = datetime('now'),
                            access_token = '',
                            refresh_token = ''
                      WHERE code_hash = ?1
                        AND consumed_at IS NULL
                        AND expires_at > ?2
                      RETURNING account_id",
                        params![hash, now_text],
                        |r| r.get(0),
                    )
                    .optional()?;
                Ok(row)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let row = conn.query_opt(
                    "UPDATE auth_link_codes
                    SET consumed_at = now(),
                        access_token = '',
                        refresh_token = ''
                  WHERE code_hash = $1
                    AND consumed_at IS NULL
                    AND expires_at > $2
                  RETURNING account_id",
                    &[&hash, &now],
                )?;
                row.map(|row| row.try_get(0))
                    .transpose()
                    .map_err(Into::into)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};
    use std::sync::{Arc, Barrier};

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
        let code = mint(&pool, &id).unwrap();
        assert_eq!(exchange(&pool, &code).unwrap(), Some(id));
    }

    #[test]
    fn exchange_is_single_use() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let code = mint(&pool, &id).unwrap();
        exchange(&pool, &code).unwrap();
        assert!(exchange(&pool, &code).unwrap().is_none());
    }

    #[test]
    fn concurrent_exchange_has_exactly_one_winner() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let code = mint(&pool, &id).unwrap();
        let barrier = Arc::new(Barrier::new(3));

        let attempts = (0..2)
            .map(|_| {
                let pool = pool.clone();
                let code = code.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    exchange(&pool, &code).unwrap()
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();

        let results = attempts
            .into_iter()
            .map(|attempt| attempt.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_none()).count(), 1);
        assert!(results
            .into_iter()
            .flatten()
            .all(|account_id| account_id == id));
    }

    #[test]
    fn exchange_rejects_unknown_code() {
        let pool = temp_pool();
        assert!(exchange(&pool, "not-a-real-code").unwrap().is_none());
    }

    #[test]
    fn mint_never_persists_credentials_and_exchange_scrubs_legacy_rows() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let code = mint(&pool, &id).unwrap();
        let hash = hash_code(&code);
        let conn = pool.get().unwrap();
        let stored: (String, String) = conn
            .query_row(
                "SELECT access_token, refresh_token FROM auth_link_codes WHERE code_hash = ?1",
                params![hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (String::new(), String::new()));

        let legacy_code = random_code();
        let legacy_hash = hash_code(&legacy_code);
        let expires_at = (Utc::now() + Duration::minutes(5)).to_rfc3339();
        conn.execute(
            "INSERT INTO auth_link_codes
             (code_hash, account_id, access_token, refresh_token, expires_at)
             VALUES (?1, ?2, 'legacy-access', 'legacy-refresh', ?3)",
            params![legacy_hash, id, expires_at],
        )
        .unwrap();
        drop(conn);

        assert_eq!(exchange(&pool, &legacy_code).unwrap(), Some(id));
        let conn = pool.get().unwrap();
        let scrubbed: (String, String) = conn
            .query_row(
                "SELECT access_token, refresh_token FROM auth_link_codes WHERE code_hash = ?1",
                params![legacy_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(scrubbed, (String::new(), String::new()));
    }

    #[test]
    fn migration_replay_scrubs_unexchanged_legacy_credentials() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let legacy_hash = hash_code(&random_code());
        let expires_at = (Utc::now() + Duration::minutes(5)).to_rfc3339();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO auth_link_codes
                 (code_hash, account_id, access_token, refresh_token, expires_at)
                 VALUES (?1, ?2, 'legacy-access', 'legacy-refresh', ?3)",
                params![legacy_hash, id, expires_at],
            )
            .unwrap();

        run_migrations(&pool).unwrap();

        let stored: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT access_token, refresh_token
                   FROM auth_link_codes
                  WHERE code_hash = ?1",
                params![legacy_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (String::new(), String::new()));
    }
}
