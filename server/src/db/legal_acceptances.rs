//! Durable Terms/Privacy acceptance ledger.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, Clone)]
pub struct LegalAcceptance<'a> {
    pub account_id: &'a str,
    pub purpose: &'a str,
    pub terms_version: &'a str,
    pub privacy_version: &'a str,
    pub terms_text_hash: &'a str,
    pub privacy_text_hash: &'a str,
    pub email_hash: Option<&'a str>,
    pub ip_hash: Option<&'a str>,
    pub user_agent_hash: Option<&'a str>,
    pub device_hash: Option<&'a str>,
    pub ip_user_agent_hash: Option<&'a str>,
    pub metadata_json: &'a str,
    pub retention_expires_at: &'a str,
}

pub fn record(pool: &DbPool, acceptance: LegalAcceptance<'_>) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let id = uuid::Uuid::new_v4().to_string();
            let accepted_at = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO legal_acceptances
                    (id, account_id, purpose, terms_version, privacy_version,
                     terms_text_hash, privacy_text_hash, email_hash, ip_hash,
                     user_agent_hash, device_hash, ip_user_agent_hash, metadata_json,
                     retention_expires_at, accepted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT(account_id, purpose, terms_version, privacy_version)
                 DO NOTHING",
                params![
                    id,
                    acceptance.account_id,
                    acceptance.purpose,
                    acceptance.terms_version,
                    acceptance.privacy_version,
                    acceptance.terms_text_hash,
                    acceptance.privacy_text_hash,
                    acceptance.email_hash,
                    acceptance.ip_hash,
                    acceptance.user_agent_hash,
                    acceptance.device_hash,
                    acceptance.ip_user_agent_hash,
                    acceptance.metadata_json,
                    acceptance.retention_expires_at,
                    accepted_at,
                ],
            )?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let id = uuid::Uuid::new_v4().to_string();
            let retention_expires_at =
                chrono::DateTime::parse_from_rfc3339(acceptance.retention_expires_at)?
                    .with_timezone(&chrono::Utc);
            let accepted_at = chrono::Utc::now();
            conn.execute(
                "INSERT INTO legal_acceptances
                    (id, account_id, purpose, terms_version, privacy_version,
                     terms_text_hash, privacy_text_hash, email_hash, ip_hash,
                     user_agent_hash, device_hash, ip_user_agent_hash, metadata_json,
                     retention_expires_at, accepted_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                 ON CONFLICT (account_id, purpose, terms_version, privacy_version)
                 DO NOTHING",
                &[
                    &id,
                    &acceptance.account_id,
                    &acceptance.purpose,
                    &acceptance.terms_version,
                    &acceptance.privacy_version,
                    &acceptance.terms_text_hash,
                    &acceptance.privacy_text_hash,
                    &acceptance.email_hash,
                    &acceptance.ip_hash,
                    &acceptance.user_agent_hash,
                    &acceptance.device_hash,
                    &acceptance.ip_user_agent_hash,
                    &acceptance.metadata_json,
                    &retention_expires_at,
                    &accepted_at,
                ],
            )?;
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{accounts::Account, open_pool, run_migrations};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-legal-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    fn acceptance<'a>(account_id: &'a str) -> LegalAcceptance<'a> {
        LegalAcceptance {
            account_id,
            purpose: "trial_terms_privacy",
            terms_version: "2026-07-09",
            privacy_version: "2026-07-09",
            terms_text_hash: "sha256:terms",
            privacy_text_hash: "sha256:privacy",
            email_hash: Some("email-hash"),
            ip_hash: Some("ip-hash"),
            user_agent_hash: Some("ua-hash"),
            device_hash: Some("device-hash"),
            ip_user_agent_hash: Some("ip-ua-hash"),
            metadata_json: r#"{"flow":"try_us"}"#,
            retention_expires_at: "2027-07-09T00:00:00Z",
        }
    }

    #[test]
    fn record_is_idempotent_for_same_account_purpose_and_versions() {
        let pool = temp_pool();
        let account = Account::create(&pool, "legal@example.com", "hash").unwrap();

        record(&pool, acceptance(&account.id)).unwrap();
        record(&pool, acceptance(&account.id)).unwrap();

        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM legal_acceptances WHERE account_id = ?1",
                params![account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn record_fails_for_unknown_account_instead_of_silently_ignoring_it() {
        let pool = temp_pool();
        let result = record(&pool, acceptance("missing-account"));
        assert!(result.is_err());
    }

    #[test]
    fn sqlite_upgrade_adds_accepted_at_without_nonconstant_default() {
        let path =
            std::env::temp_dir().join(format!("bluey-legal-upgrade-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        {
            let conn = pool.get().unwrap();
            conn.execute_batch(
                "CREATE TABLE legal_acceptances (
                    id TEXT PRIMARY KEY,
                    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                    purpose TEXT NOT NULL,
                    terms_version TEXT NOT NULL,
                    privacy_version TEXT NOT NULL,
                    terms_text_hash TEXT NOT NULL DEFAULT '',
                    privacy_text_hash TEXT NOT NULL DEFAULT '',
                    email_hash TEXT,
                    ip_hash TEXT,
                    user_agent_hash TEXT,
                    device_hash TEXT,
                    ip_user_agent_hash TEXT,
                    metadata_json TEXT NOT NULL DEFAULT '{}',
                    retention_expires_at DATETIME,
                    created_at DATETIME NOT NULL DEFAULT (datetime('now')),
                    UNIQUE(account_id, purpose, terms_version, privacy_version)
                );",
            )
            .unwrap();
        }

        run_migrations(&pool).unwrap();
        let account = Account::create(&pool, "legal-upgrade@example.com", "hash").unwrap();
        record(&pool, acceptance(&account.id)).unwrap();

        let conn = pool.get().unwrap();
        let accepted_at: String = conn
            .query_row(
                "SELECT accepted_at FROM legal_acceptances WHERE account_id = ?1",
                params![account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!accepted_at.is_empty());
    }
}
