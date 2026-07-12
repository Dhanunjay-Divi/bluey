//! Diagnostic log chunk index.
//!
//! The log body lives in bounded local files or private R2/S3 objects. This
//! table stores only routing metadata so support can find relevant diagnostics
//! by account/session/kind without putting raw user content in Postgres.

use anyhow::Result;
use rusqlite::params;
use std::collections::HashSet;

use crate::db::DbPool;

#[derive(Debug, Clone)]
pub struct DiagnosticLogChunkInput {
    pub id: Option<String>,
    pub account_id: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub session_code: Option<String>,
    pub kind: String,
    pub storage: String,
    pub object_key: Option<String>,
    pub local_path: Option<String>,
    pub bytes: i64,
    pub sha256: Option<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub metadata_json: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticLogChunk {
    pub id: String,
    pub account_id: Option<String>,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub session_code: Option<String>,
    pub kind: String,
    pub storage: String,
    pub object_key: Option<String>,
    pub local_path: Option<String>,
    pub bytes: i64,
    pub sha256: Option<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub metadata_json: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DiagnosticObjectRef {
    pub id: String,
    pub object_key: String,
    pub bytes: i64,
    pub sha256: Option<String>,
}

pub fn record_chunk(pool: &DbPool, input: DiagnosticLogChunkInput) -> Result<String> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_chunk_sqlite(pool, input),
        DbPool::Postgres(_) => record_chunk_postgres(pool, input),
    })
}

pub fn recent_for_account(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<DiagnosticLogChunk>> {
    let limit = limit.clamp(1, 200);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => recent_for_account_sqlite(pool, account_id, limit),
        DbPool::Postgres(_) => recent_for_account_postgres(pool, account_id, limit),
    })
}

pub fn object_refs_for_account(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<DiagnosticObjectRef>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => object_refs_for_account_sqlite(pool, account_id),
        DbPool::Postgres(_) => object_refs_for_account_postgres(pool, account_id),
    })
}

pub fn delete_expired_before(pool: &DbPool, now_ms: i64) -> Result<usize> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => delete_expired_before_sqlite(pool, now_ms),
        DbPool::Postgres(_) => delete_expired_before_postgres(pool, now_ms),
    })
}

fn record_chunk_sqlite(pool: &DbPool, input: DiagnosticLogChunkInput) -> Result<String> {
    let conn = pool.get()?;
    let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    conn.execute(
        "INSERT OR REPLACE INTO diagnostic_log_chunks
            (id, account_id, workspace_id, session_id, session_code, kind, storage,
             object_key, local_path, bytes, sha256, created_at_ms, expires_at_ms, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            id,
            input.account_id,
            input.workspace_id,
            input.session_id,
            input.session_code,
            input.kind,
            input.storage,
            input.object_key,
            input.local_path,
            input.bytes,
            input.sha256,
            input.created_at_ms,
            input.expires_at_ms,
            input.metadata_json.to_string(),
        ],
    )?;
    Ok(id)
}

fn record_chunk_postgres(pool: &DbPool, input: DiagnosticLogChunkInput) -> Result<String> {
    let mut conn = pool.get_pg()?;
    let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let metadata_json = input.metadata_json.to_string();
    conn.execute(
        "INSERT INTO diagnostic_log_chunks
            (id, account_id, workspace_id, session_id, session_code, kind, storage,
             object_key, local_path, bytes, sha256, created_at_ms, expires_at_ms, metadata_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT (id) DO UPDATE SET
             account_id = EXCLUDED.account_id,
             workspace_id = EXCLUDED.workspace_id,
             session_id = EXCLUDED.session_id,
             session_code = EXCLUDED.session_code,
             kind = EXCLUDED.kind,
             storage = EXCLUDED.storage,
             object_key = EXCLUDED.object_key,
             local_path = EXCLUDED.local_path,
             bytes = EXCLUDED.bytes,
             sha256 = EXCLUDED.sha256,
             created_at_ms = EXCLUDED.created_at_ms,
             expires_at_ms = EXCLUDED.expires_at_ms,
             metadata_json = EXCLUDED.metadata_json",
        &[
            &id,
            &input.account_id,
            &input.workspace_id,
            &input.session_id,
            &input.session_code,
            &input.kind,
            &input.storage,
            &input.object_key,
            &input.local_path,
            &input.bytes,
            &input.sha256,
            &input.created_at_ms,
            &input.expires_at_ms,
            &metadata_json,
        ],
    )?;
    Ok(id)
}

fn recent_for_account_sqlite(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<DiagnosticLogChunk>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, account_id, workspace_id, session_id, session_code, kind, storage,
                object_key, local_path, bytes, sha256, created_at_ms, expires_at_ms, metadata_json
           FROM diagnostic_log_chunks
          WHERE account_id = ?1
          ORDER BY created_at_ms DESC
          LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![account_id, limit], row_to_chunk_sqlite)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn recent_for_account_postgres(
    pool: &DbPool,
    account_id: &str,
    limit: i64,
) -> Result<Vec<DiagnosticLogChunk>> {
    let mut conn = pool.get_pg()?;
    let rows = conn.query(
        "SELECT id, account_id, workspace_id, session_id, session_code, kind, storage,
                object_key, local_path, bytes, sha256, created_at_ms, expires_at_ms, metadata_json
           FROM diagnostic_log_chunks
          WHERE account_id = $1
          ORDER BY created_at_ms DESC
          LIMIT $2",
        &[&account_id, &limit],
    )?;
    rows.into_iter().map(row_to_chunk_postgres).collect()
}

fn object_refs_for_account_sqlite(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<DiagnosticObjectRef>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, object_key, bytes, sha256
           FROM diagnostic_log_chunks
          WHERE account_id = ?1
            AND storage = 'r2'
            AND object_key IS NOT NULL
            AND object_key <> ''",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(DiagnosticObjectRef {
            id: row.get(0)?,
            object_key: row.get(1)?,
            bytes: row.get(2)?,
            sha256: row.get(3)?,
        })
    })?;
    let mut refs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut seen = refs
        .iter()
        .map(|reference| reference.object_key.clone())
        .collect::<HashSet<_>>();
    let mut stmt = conn.prepare(
        "SELECT id, object_key, size_bytes, sha256
           FROM object_uploads
          WHERE account_id = ?1 AND object_kind = 'session_audit' AND state <> 'deleted'",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(DiagnosticObjectRef {
            id: format!("object-upload:{}", row.get::<_, String>(0)?),
            object_key: row.get(1)?,
            bytes: row.get(2)?,
            sha256: row.get(3)?,
        })
    })?;
    for row in rows {
        let reference = row?;
        if seen.insert(reference.object_key.clone()) {
            refs.push(reference);
        }
    }
    Ok(refs)
}

fn object_refs_for_account_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<DiagnosticObjectRef>> {
    let mut conn = pool.get_pg()?;
    let rows = conn.query(
        "SELECT id, object_key, bytes, sha256
           FROM diagnostic_log_chunks
          WHERE account_id = $1
            AND storage = 'r2'
            AND object_key IS NOT NULL
            AND object_key <> ''",
        &[&account_id],
    )?;
    let mut refs = rows
        .into_iter()
        .map(|row| {
            Ok(DiagnosticObjectRef {
                id: row.try_get(0)?,
                object_key: row.try_get(1)?,
                bytes: row.try_get(2)?,
                sha256: row.try_get(3)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut seen = refs
        .iter()
        .map(|reference| reference.object_key.clone())
        .collect::<HashSet<_>>();
    let rows = conn.query(
        "SELECT id, object_key, size_bytes, sha256
           FROM object_uploads
          WHERE account_id = $1 AND object_kind = 'session_audit' AND state <> 'deleted'",
        &[&account_id],
    )?;
    for row in rows {
        let reference = DiagnosticObjectRef {
            id: format!("object-upload:{}", row.try_get::<_, String>(0)?),
            object_key: row.try_get(1)?,
            bytes: row.try_get(2)?,
            sha256: row.try_get(3)?,
        };
        if seen.insert(reference.object_key.clone()) {
            refs.push(reference);
        }
    }
    Ok(refs)
}

fn delete_expired_before_sqlite(pool: &DbPool, now_ms: i64) -> Result<usize> {
    let conn = pool.get()?;
    Ok(conn.execute(
        "DELETE FROM diagnostic_log_chunks WHERE expires_at_ms < ?1",
        params![now_ms],
    )?)
}

fn delete_expired_before_postgres(pool: &DbPool, now_ms: i64) -> Result<usize> {
    let mut conn = pool.get_pg()?;
    let deleted = conn.execute(
        "DELETE FROM diagnostic_log_chunks WHERE expires_at_ms < $1",
        &[&now_ms],
    )?;
    Ok(deleted as usize)
}

fn row_to_chunk_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<DiagnosticLogChunk> {
    let metadata: String = row.get(13)?;
    Ok(DiagnosticLogChunk {
        id: row.get(0)?,
        account_id: row.get(1)?,
        workspace_id: row.get(2)?,
        session_id: row.get(3)?,
        session_code: row.get(4)?,
        kind: row.get(5)?,
        storage: row.get(6)?,
        object_key: row.get(7)?,
        local_path: row.get(8)?,
        bytes: row.get(9)?,
        sha256: row.get(10)?,
        created_at_ms: row.get(11)?,
        expires_at_ms: row.get(12)?,
        metadata_json: parse_json(&metadata),
    })
}

fn row_to_chunk_postgres(row: postgres::Row) -> Result<DiagnosticLogChunk> {
    let metadata: String = row.try_get(13)?;
    Ok(DiagnosticLogChunk {
        id: row.try_get(0)?,
        account_id: row.try_get(1)?,
        workspace_id: row.try_get(2)?,
        session_id: row.try_get(3)?,
        session_code: row.try_get(4)?,
        kind: row.try_get(5)?,
        storage: row.try_get(6)?,
        object_key: row.try_get(7)?,
        local_path: row.try_get(8)?,
        bytes: row.try_get(9)?,
        sha256: row.try_get(10)?,
        created_at_ms: row.try_get(11)?,
        expires_at_ms: row.try_get(12)?,
        metadata_json: parse_json(&metadata),
    })
}

fn parse_json(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> crate::db::DbPool {
        let path =
            std::env::temp_dir().join(format!("bluey-diagnostic-logs-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts(id, email, password_hash)
             VALUES ('acct_1', 'diag@example.test', 'hash')",
            [],
        )
        .unwrap();
        drop(conn);
        pool
    }

    #[test]
    fn records_account_scoped_chunk_metadata_without_body() {
        let pool = test_pool();
        let id = record_chunk(
            &pool,
            DiagnosticLogChunkInput {
                id: Some("diag_1".into()),
                account_id: Some("acct_1".into()),
                workspace_id: Some("workspace_1".into()),
                session_id: Some("session_1".into()),
                session_code: Some("ABC12345".into()),
                kind: "desktop_redacted_bundle".into(),
                storage: "r2".into(),
                object_key: Some("prod/logs/accounts/acct_1/date-2026-07-05/chunk.log".into()),
                local_path: None,
                bytes: 42,
                sha256: Some("abc".into()),
                created_at_ms: 1000,
                expires_at_ms: 2000,
                metadata_json: serde_json::json!({"redacted": true}),
            },
        )
        .unwrap();

        assert_eq!(id, "diag_1");
        let chunks = recent_for_account(&pool, "acct_1", 10).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, "desktop_redacted_bundle");
        assert_eq!(chunks[0].bytes, 42);
        assert_eq!(chunks[0].metadata_json["redacted"], true);
        assert!(recent_for_account(&pool, "acct_2", 10).unwrap().is_empty());
    }

    #[test]
    fn prunes_expired_index_rows() {
        let pool = test_pool();
        for (id, expires_at_ms) in [("old", 999), ("new", 2000)] {
            record_chunk(
                &pool,
                DiagnosticLogChunkInput {
                    id: Some(id.into()),
                    account_id: Some("acct_1".into()),
                    workspace_id: None,
                    session_id: None,
                    session_code: None,
                    kind: "server_operational_bundle".into(),
                    storage: "r2".into(),
                    object_key: Some(format!("prod/logs/api/{id}.tar.gz")),
                    local_path: None,
                    bytes: 1,
                    sha256: None,
                    created_at_ms: 1000,
                    expires_at_ms,
                    metadata_json: serde_json::json!({}),
                },
            )
            .unwrap();
        }

        assert_eq!(delete_expired_before(&pool, 1000).unwrap(), 1);
        let chunks = recent_for_account(&pool, "acct_1", 10).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].id, "new");
    }
}
