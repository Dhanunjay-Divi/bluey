//! Short-lived, account-bound capability storage for Jobs -> Bluey desktop handoffs.
//!
//! Raw nonces are returned once and never stored. Snapshot payloads use the
//! existing authenticated Jobs encryption envelope so a database read does not
//! expose submitted-application context.

use anyhow::{Context, Result};
use base64::Engine;
use rand::{rngs::OsRng, RngCore};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{jobs, DbPool};

pub const BLUEY_DESKTOP_AUDIENCE: &str = "bluey-desktop-interview-prep-v1";
pub const HANDOFF_TTL_MS: i64 = 90_000;
pub const NONCE_BYTES: usize = 32;
pub const ENCODED_NONCE_LEN: usize = 43;
const MAX_SNAPSHOT_BYTES: usize = 128 * 1024;
const RETIRED_ROW_RETENTION_MS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedHandoff {
    pub nonce: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RedeemedHandoff {
    pub application_id: String,
    pub snapshot: Value,
}

pub fn issue(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    audience: &str,
    snapshot: &Value,
) -> Result<IssuedHandoff> {
    issue_at(
        pool,
        account_id,
        application_id,
        audience,
        snapshot,
        jobs::now_ms(),
        HANDOFF_TTL_MS,
    )
}

pub fn redeem(
    pool: &DbPool,
    account_id: &str,
    audience: &str,
    nonce: &str,
) -> Result<Option<RedeemedHandoff>> {
    redeem_at(pool, account_id, audience, nonce, jobs::now_ms())
}

pub fn valid_nonce(nonce: &str) -> bool {
    nonce.len() == ENCODED_NONCE_LEN
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        && base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(nonce)
            .is_ok_and(|decoded| decoded.len() == NONCE_BYTES)
}

fn issue_at(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    audience: &str,
    snapshot: &Value,
    now_ms: i64,
    ttl_ms: i64,
) -> Result<IssuedHandoff> {
    validate_binding("account_id", account_id, 240)?;
    validate_binding("application_id", application_id, 240)?;
    validate_audience(audience)?;
    if !(1..=HANDOFF_TTL_MS).contains(&ttl_ms) {
        anyhow::bail!("invalid Jobs handoff lifetime")
    }
    let snapshot_bytes = serde_json::to_vec(snapshot).context("serialize Jobs handoff snapshot")?;
    if snapshot_bytes.len() > MAX_SNAPSHOT_BYTES {
        anyhow::bail!("Jobs handoff snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes")
    }
    let encrypted_snapshot = jobs::to_json(snapshot, "Jobs desktop handoff snapshot")?;
    let nonce = random_nonce()?;
    let nonce_hash = hash_nonce(&nonce);
    let expires_at_ms = now_ms
        .checked_add(ttl_ms)
        .context("Jobs handoff expiry overflow")?;
    let retire_before_ms = now_ms.saturating_sub(RETIRED_ROW_RETENTION_MS);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM jobs_bluey_handoffs
                  WHERE expires_at_ms < ?1
                     OR (consumed_at_ms IS NOT NULL AND consumed_at_ms < ?1)",
                params![retire_before_ms],
            )?;
            let inserted = tx.execute(
                "INSERT INTO jobs_bluey_handoffs (
                    nonce_hash, account_id, application_id, audience, snapshot_json,
                    created_at_ms, expires_at_ms, consumed_at_ms
                 )
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL
                   FROM jobs_applications
                  WHERE id = ?3 AND account_id = ?2",
                params![
                    nonce_hash,
                    account_id,
                    application_id,
                    audience,
                    encrypted_snapshot,
                    now_ms,
                    expires_at_ms,
                ],
            )?;
            if inserted != 1 {
                anyhow::bail!("Jobs handoff application is not owned by this account")
            }
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM jobs_bluey_handoffs
                  WHERE expires_at_ms < $1
                     OR (consumed_at_ms IS NOT NULL AND consumed_at_ms < $1)",
                &[&retire_before_ms],
            )?;
            let inserted = tx.execute(
                "INSERT INTO jobs_bluey_handoffs (
                    nonce_hash, account_id, application_id, audience, snapshot_json,
                    created_at_ms, expires_at_ms, consumed_at_ms
                 )
                 SELECT $1, $2, $3, $4, $5, $6, $7, NULL
                   FROM jobs_applications
                  WHERE id = $3 AND account_id = $2",
                &[
                    &nonce_hash,
                    &account_id,
                    &application_id,
                    &audience,
                    &encrypted_snapshot,
                    &now_ms,
                    &expires_at_ms,
                ],
            )?;
            if inserted != 1 {
                anyhow::bail!("Jobs handoff application is not owned by this account")
            }
            tx.commit()?;
            Ok::<(), anyhow::Error>(())
        }
    })?;

    Ok(IssuedHandoff {
        nonce,
        expires_at_ms,
    })
}

fn redeem_at(
    pool: &DbPool,
    account_id: &str,
    audience: &str,
    nonce: &str,
    now_ms: i64,
) -> Result<Option<RedeemedHandoff>> {
    if !valid_nonce(nonce)
        || validate_binding("account_id", account_id, 240).is_err()
        || validate_audience(audience).is_err()
    {
        return Ok(None);
    }
    let nonce_hash = hash_nonce(nonce);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let row: Option<(String, String)> = tx
                .query_row(
                    "UPDATE jobs_bluey_handoffs
                        SET consumed_at_ms = ?1
                      WHERE nonce_hash = ?2
                        AND account_id = ?3
                        AND audience = ?4
                        AND consumed_at_ms IS NULL
                        AND expires_at_ms > ?1
                      RETURNING application_id, snapshot_json",
                    params![now_ms, nonce_hash, account_id, audience],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let redeemed = row
                .map(|(application_id, raw)| {
                    let snapshot = jobs::parse_json(raw, "Jobs desktop handoff snapshot")?;
                    Ok::<RedeemedHandoff, anyhow::Error>(RedeemedHandoff {
                        application_id,
                        snapshot,
                    })
                })
                .transpose()?;
            tx.commit()?;
            Ok::<Option<RedeemedHandoff>, anyhow::Error>(redeemed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "UPDATE jobs_bluey_handoffs
                    SET consumed_at_ms = $1
                  WHERE nonce_hash = $2
                    AND account_id = $3
                    AND audience = $4
                    AND consumed_at_ms IS NULL
                    AND expires_at_ms > $1
                  RETURNING application_id, snapshot_json",
                &[&now_ms, &nonce_hash, &account_id, &audience],
            )?;
            let redeemed = row
                .map(|row| {
                    let application_id: String = row.try_get(0)?;
                    let raw: String = row.try_get(1)?;
                    let snapshot = jobs::parse_json(raw, "Jobs desktop handoff snapshot")?;
                    Ok::<_, anyhow::Error>(RedeemedHandoff {
                        application_id,
                        snapshot,
                    })
                })
                .transpose()?;
            tx.commit()?;
            Ok::<Option<RedeemedHandoff>, anyhow::Error>(redeemed)
        }
    })
}

fn random_nonce() -> Result<String> {
    let mut bytes = [0_u8; NONCE_BYTES];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|error| anyhow::anyhow!("generate Jobs handoff nonce: {error}"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn hash_nonce(nonce: &str) -> String {
    hex::encode(Sha256::digest(nonce.as_bytes()))
}

fn validate_binding(label: &str, value: &str, max: usize) -> Result<()> {
    if value.is_empty()
        || value.len() > max
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        anyhow::bail!("invalid Jobs handoff {label}")
    }
    Ok(())
}

fn validate_audience(audience: &str) -> Result<()> {
    if audience.is_empty()
        || audience.len() > 64
        || !audience
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        anyhow::bail!("invalid Jobs handoff audience")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{accounts::Account, open_pool, run_migrations};
    use serde_json::json;

    fn fixture() -> (DbPool, String, String, String) {
        let path =
            std::env::temp_dir().join(format!("bluey-jobs-handoff-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        let owner = Account::create(&pool, "handoff-owner@example.com", "stub")
            .unwrap()
            .id;
        let other = Account::create(&pool, "handoff-other@example.com", "stub")
            .unwrap()
            .id;
        let application_id = "application-handoff-1".to_string();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, canonical_url,
                company, title, location, match_score, status, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, 'handoff-job', '{}', 'test', '', 'Acme', 'Engineer',
                       '', 100, 'matched', 1, 1)",
            params!["job-handoff-1", owner],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_applications (
                id, account_id, job_id, state, application_json, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, 'job-handoff-1', 'submitted', '{}', 1, 1)",
            params![application_id, owner],
        )
        .unwrap();
        drop(conn);
        (pool, owner, other, application_id)
    }

    #[test]
    fn issue_redeem_is_random_hashed_encrypted_and_single_use() {
        let (pool, owner, _, application_id) = fixture();
        let snapshot = json!({"application_id": application_id, "company": "Acme"});
        let issued = issue_at(
            &pool,
            &owner,
            &application_id,
            BLUEY_DESKTOP_AUDIENCE,
            &snapshot,
            10_000,
            HANDOFF_TTL_MS,
        )
        .unwrap();
        assert!(valid_nonce(&issued.nonce));
        assert_eq!(issued.expires_at_ms, 100_000);

        let (stored_hash, stored_snapshot): (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT nonce_hash, snapshot_json FROM jobs_bluey_handoffs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored_hash, hash_nonce(&issued.nonce));
        assert!(!stored_hash.contains(&issued.nonce));
        assert!(stored_snapshot.starts_with("bluey-jobs:v1:"));
        assert!(!stored_snapshot.contains("Acme"));

        let redeemed = redeem_at(&pool, &owner, BLUEY_DESKTOP_AUDIENCE, &issued.nonce, 10_001)
            .unwrap()
            .unwrap();
        assert_eq!(redeemed.application_id, application_id);
        assert_eq!(redeemed.snapshot, snapshot);
        assert!(
            redeem_at(&pool, &owner, BLUEY_DESKTOP_AUDIENCE, &issued.nonce, 10_002,)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn account_audience_and_expiry_checks_do_not_consume_the_nonce() {
        let (pool, owner, other, application_id) = fixture();
        assert!(issue_at(
            &pool,
            &other,
            &application_id,
            BLUEY_DESKTOP_AUDIENCE,
            &json!({"safe": true}),
            1_000,
            HANDOFF_TTL_MS,
        )
        .unwrap_err()
        .to_string()
        .contains("not owned"));
        let issued = issue_at(
            &pool,
            &owner,
            &application_id,
            BLUEY_DESKTOP_AUDIENCE,
            &json!({"safe": true}),
            1_000,
            HANDOFF_TTL_MS,
        )
        .unwrap();

        assert!(
            redeem_at(&pool, &other, BLUEY_DESKTOP_AUDIENCE, &issued.nonce, 1_001,)
                .unwrap()
                .is_none()
        );
        assert!(
            redeem_at(&pool, &owner, "wrong-audience", &issued.nonce, 1_002)
                .unwrap()
                .is_none()
        );
        assert!(
            redeem_at(&pool, &owner, BLUEY_DESKTOP_AUDIENCE, &issued.nonce, 91_001,)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn malformed_nonce_and_oversized_snapshot_are_rejected() {
        let (pool, owner, _, application_id) = fixture();
        assert!(!valid_nonce("short"));
        assert!(
            redeem_at(&pool, &owner, BLUEY_DESKTOP_AUDIENCE, "short", 1,)
                .unwrap()
                .is_none()
        );
        let oversized = json!({"text": "x".repeat(MAX_SNAPSHOT_BYTES)});
        assert!(issue_at(
            &pool,
            &owner,
            &application_id,
            BLUEY_DESKTOP_AUDIENCE,
            &oversized,
            1,
            HANDOFF_TTL_MS,
        )
        .unwrap_err()
        .to_string()
        .contains("exceeds"));
    }
}
