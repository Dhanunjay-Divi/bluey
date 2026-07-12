//! Vector store: SQLite-backed chunk storage + in-memory cosine similarity search.
//!
//! v0.1 fallback: embeddings stored as BLOB in SQLite, cosine similarity computed
//! in Rust. Follow-up: swap to sqlite-vec virtual tables for native ANN search.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

const LEGACY_UNSCOPED_ACCOUNT_ID: &str = "__bluey_legacy_unscoped__";

/// A search result from the vector store.
#[derive(Debug, Clone)]
pub struct RagHit {
    pub session_id: String,
    pub chunk_text: String,
    pub score: f32,
}

/// Immutable owner scope for local RAG reads and writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RagScope {
    account_id: String,
    workspace_id: Option<String>,
}

impl RagScope {
    pub fn new(account_id: &str, workspace_id: Option<&str>) -> Result<Self> {
        let account_id = account_id.trim();
        anyhow::ensure!(!account_id.is_empty(), "RAG account id cannot be empty");
        anyhow::ensure!(
            account_id != LEGACY_UNSCOPED_ACCOUNT_ID,
            "reserved RAG account id"
        );
        let workspace_id = workspace_id
            .map(str::trim)
            .filter(|workspace_id| !workspace_id.is_empty())
            .map(ToOwned::to_owned);
        Ok(Self {
            account_id: account_id.to_string(),
            workspace_id,
        })
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }
}

/// Vector store backed by SQLite with in-memory cosine similarity.
pub struct VectorStore {
    conn: Connection,
    dim: usize,
}

impl VectorStore {
    /// Open or create the vector store database.
    pub fn open(path: &Path, dim: usize) -> Result<Self> {
        let mut conn = if path.as_os_str() == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(path)?
        };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Self::run_migrations(&mut conn)?;
        Ok(Self { conn, dim })
    }

    fn run_migrations(conn: &mut Connection) -> Result<()> {
        let tx = conn.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS rag_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id TEXT NOT NULL,
                workspace_id TEXT,
                session_id TEXT NOT NULL,
                text TEXT NOT NULL,
                start_char INTEGER NOT NULL,
                end_char INTEGER NOT NULL,
                ts_ms INTEGER NOT NULL DEFAULT 0
            );",
        )
        .context("failed to run RAG migrations")?;

        if !table_has_column(&tx, "rag_chunks", "account_id")? {
            tx.execute_batch(
                "ALTER TABLE rag_chunks
                 ADD COLUMN account_id TEXT NOT NULL
                 DEFAULT '__bluey_legacy_unscoped__';",
            )?;
        }
        if !table_has_column(&tx, "rag_chunks", "workspace_id")? {
            tx.execute_batch("ALTER TABLE rag_chunks ADD COLUMN workspace_id TEXT;")?;
        }

        // Old databases did not record an owner. Keep those rows inaccessible
        // until the daemon verifies the saved meeting owner and claims them.
        tx.execute(
            "UPDATE rag_chunks
             SET account_id = ?1
             WHERE account_id IS NULL OR trim(account_id) = ''",
            params![LEGACY_UNSCOPED_ACCOUNT_ID],
        )?;
        tx.execute_batch(
            "DROP INDEX IF EXISTS idx_rag_chunks_session;
             CREATE INDEX IF NOT EXISTS idx_rag_chunks_owner_session
             ON rag_chunks(account_id, workspace_id, session_id);
             CREATE TABLE IF NOT EXISTS rag_embeddings (
                 chunk_id INTEGER PRIMARY KEY REFERENCES rag_chunks(id) ON DELETE CASCADE,
                 embedding BLOB NOT NULL
             );",
        )?;
        tx.commit().context("failed to commit RAG migrations")?;
        Ok(())
    }

    /// Index a chunk with its embedding.
    pub fn index(
        &self,
        scope: &RagScope,
        session_id: &str,
        chunk: &crate::Chunk,
        embedding: &[f32],
    ) -> Result<()> {
        anyhow::ensure!(
            embedding.len() == self.dim,
            "embedding dim mismatch: expected {}, got {}",
            self.dim,
            embedding.len()
        );
        self.conn.execute(
            "INSERT INTO rag_chunks
             (account_id, workspace_id, session_id, text, start_char, end_char, ts_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                scope.account_id(),
                scope.workspace_id(),
                session_id,
                chunk.text,
                chunk.start_char,
                chunk.end_char,
                now_ms()
            ],
        )?;
        let chunk_id = self.conn.last_insert_rowid();
        let blob = embedding_to_blob(embedding);
        self.conn.execute(
            "INSERT INTO rag_embeddings (chunk_id, embedding) VALUES (?1, ?2)",
            params![chunk_id, blob],
        )?;
        Ok(())
    }

    /// Query the store for the most similar chunks.
    ///
    /// Uses a bounded min-heap of size `limit` to avoid sorting all N rows
    /// when only `limit` matter. For typical k=10 queries this is 50-100x
    /// cheaper than the previous "sort all then truncate" approach.
    ///
    /// Note: this is still O(N) over the embedding scan because we do not
    /// have an index. Past ~50k chunks per database the scan dominates and
    /// we should switch to a vector index (sqlite-vec or usearch). See
    /// `docs/work/PHASE-3-ROUND-14-PLAN.md`.
    pub fn query(
        &self,
        scope: &RagScope,
        query_embedding: &[f32],
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<RagHit>> {
        anyhow::ensure!(
            query_embedding.len() == self.dim,
            "query embedding dim mismatch"
        );

        if limit == 0 {
            return Ok(Vec::new());
        }

        // Bounded min-heap: keep the top `limit` items by score. We invert
        // the score with `Reverse(...)` so BinaryHeap (max-heap) acts as a
        // min-heap on the score, letting us drop the lowest-scoring item
        // when a better one arrives.
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;

        // f32 does not implement Ord; use OrderedFloat-style wrapper here.
        // We sidestep adding the ordered_float crate by using a tiny tuple
        // wrapper that converts NaN to f32::NEG_INFINITY for ordering.
        #[derive(PartialEq)]
        struct OrdF32(f32);
        impl Eq for OrdF32 {}
        impl PartialOrd for OrdF32 {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for OrdF32 {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                let a = if self.0.is_nan() {
                    f32::NEG_INFINITY
                } else {
                    self.0
                };
                let b = if other.0.is_nan() {
                    f32::NEG_INFINITY
                } else {
                    other.0
                };
                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
            }
        }

        let mut heap: BinaryHeap<Reverse<(OrdF32, String, String)>> =
            BinaryHeap::with_capacity(limit + 1);

        let mut push_row = |sid: String, text: String, blob: Vec<u8>| {
            let emb = blob_to_embedding(&blob);
            let score = cosine_similarity(query_embedding, &emb);
            heap.push(Reverse((OrdF32(score), sid, text)));
            if heap.len() > limit {
                heap.pop();
            }
        };

        match session_id {
            Some(sid) => {
                let mut stmt = self.conn.prepare(
                    "SELECT c.session_id, c.text, e.embedding
                     FROM rag_chunks c
                     JOIN rag_embeddings e ON e.chunk_id = c.id
                     WHERE c.account_id = ?1
                       AND c.workspace_id IS ?2
                       AND c.session_id = ?3",
                )?;
                let mut rows =
                    stmt.query(params![scope.account_id(), scope.workspace_id(), sid])?;
                while let Some(row) = rows.next()? {
                    push_row(row.get(0)?, row.get(1)?, row.get(2)?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT c.session_id, c.text, e.embedding
                     FROM rag_chunks c
                     JOIN rag_embeddings e ON e.chunk_id = c.id
                     WHERE c.account_id = ?1 AND c.workspace_id IS ?2",
                )?;
                let mut rows = stmt.query(params![scope.account_id(), scope.workspace_id()])?;
                while let Some(row) = rows.next()? {
                    push_row(row.get(0)?, row.get(1)?, row.get(2)?);
                }
            }
        }

        // Drain the heap into a vec sorted by score descending.
        let mut out: Vec<RagHit> = heap
            .into_iter()
            .map(|Reverse((OrdF32(score), session_id, chunk_text))| RagHit {
                session_id,
                chunk_text,
                score,
            })
            .collect();
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }

    /// Delete all chunks and embeddings for an account-owned session.
    ///
    /// A session can have rows from more than one workspace after a workspace
    /// switch, so deletion intentionally spans workspaces within the account.
    pub fn delete_session(&self, scope: &RagScope, session_id: &str) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM rag_chunks WHERE account_id = ?1 AND session_id = ?2",
            params![scope.account_id(), session_id],
        )?;
        Ok(deleted)
    }

    /// Claim pre-migration rows after the caller verifies session ownership.
    pub fn claim_legacy_session(&self, scope: &RagScope, session_id: &str) -> Result<usize> {
        let claimed = self.conn.execute(
            "UPDATE rag_chunks
             SET account_id = ?1, workspace_id = ?2
             WHERE account_id = ?3 AND session_id = ?4",
            params![
                scope.account_id(),
                scope.workspace_id(),
                LEGACY_UNSCOPED_ACCOUNT_ID,
                session_id
            ],
        )?;
        Ok(claimed)
    }

    /// Number of indexed chunks visible to an account/workspace scope.
    pub fn chunk_count(&self, scope: &RagScope) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM rag_chunks
             WHERE account_id = ?1 AND workspace_id IS ?2",
            params![scope.account_id(), scope.workspace_id()],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (ai, bi) in a.iter().zip(b.iter()) {
        dot += ai * bi;
        norm_a += ai * ai;
        norm_b += bi * bi;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chunk;

    fn mem_store(dim: usize) -> VectorStore {
        VectorStore::open(Path::new(":memory:"), dim).unwrap()
    }

    fn scope(account_id: &str, workspace_id: Option<&str>) -> RagScope {
        RagScope::new(account_id, workspace_id).unwrap()
    }

    fn fake_embedding(dim: usize, seed: f32) -> Vec<f32> {
        (0..dim).map(|i| ((i as f32) * seed).sin()).collect()
    }

    #[test]
    fn vector_store_index_and_query() {
        let dim = 8;
        let store = mem_store(dim);
        let scope = scope("account-a", Some("workspace-main"));

        for i in 0..5 {
            let chunk = Chunk {
                text: format!("chunk {i}"),
                start_char: i * 10,
                end_char: (i + 1) * 10,
            };
            let emb = fake_embedding(dim, (i + 1) as f32 * 0.3);
            store.index(&scope, "session-1", &chunk, &emb).unwrap();
        }

        let query = fake_embedding(dim, 0.9); // close to seed=0.9 (i=2, seed=(2+1)*0.3=0.9)
        let results = store.query(&scope, &query, 3, None).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].score >= results[1].score);
        assert!(results[1].score >= results[2].score);
        assert_eq!(results[0].chunk_text, "chunk 2");
    }

    #[test]
    fn vector_store_session_filter() {
        let dim = 4;
        let store = mem_store(dim);
        let scope = scope("account-a", None);

        let emb = vec![1.0, 0.0, 0.0, 0.0];
        store
            .index(
                &scope,
                "s1",
                &Chunk {
                    text: "s1 chunk".into(),
                    start_char: 0,
                    end_char: 8,
                },
                &emb,
            )
            .unwrap();
        store
            .index(
                &scope,
                "s2",
                &Chunk {
                    text: "s2 chunk".into(),
                    start_char: 0,
                    end_char: 8,
                },
                &emb,
            )
            .unwrap();

        let results = store.query(&scope, &emb, 10, Some("s1")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "s1");

        let results = store.query(&scope, &emb, 10, None).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn vector_store_isolates_accounts_and_workspaces() {
        let store = mem_store(4);
        let account_a = scope("account-a", Some("workspace-main"));
        let account_a_other = scope("account-a", Some("workspace-other"));
        let account_b = scope("account-b", Some("workspace-main"));
        let emb = vec![1.0, 0.0, 0.0, 0.0];

        for (scope, text) in [
            (&account_a, "account a main"),
            (&account_a_other, "account a other"),
            (&account_b, "account b main"),
        ] {
            store
                .index(
                    scope,
                    "shared-session-id",
                    &Chunk {
                        text: text.into(),
                        start_char: 0,
                        end_char: text.len(),
                    },
                    &emb,
                )
                .unwrap();
        }

        let account_a_hits = store.query(&account_a, &emb, 10, None).unwrap();
        assert_eq!(account_a_hits.len(), 1);
        assert_eq!(account_a_hits[0].chunk_text, "account a main");

        let account_a_other_hits = store.query(&account_a_other, &emb, 10, None).unwrap();
        assert_eq!(account_a_other_hits.len(), 1);
        assert_eq!(account_a_other_hits[0].chunk_text, "account a other");

        let account_b_hits = store.query(&account_b, &emb, 10, None).unwrap();
        assert_eq!(account_b_hits.len(), 1);
        assert_eq!(account_b_hits[0].chunk_text, "account b main");
    }

    #[test]
    fn vector_store_delete_is_account_scoped_and_cascades() {
        let dim = 4;
        let store = mem_store(dim);
        let account_a = scope("account-a", Some("workspace-main"));
        let account_a_other = scope("account-a", Some("workspace-other"));
        let account_b = scope("account-b", Some("workspace-main"));
        let emb = vec![1.0, 0.0, 0.0, 0.0];

        for (scope, text) in [
            (&account_a, "delete a main"),
            (&account_a_other, "delete a other"),
            (&account_b, "keep b"),
        ] {
            store
                .index(
                    scope,
                    "shared-session-id",
                    &Chunk {
                        text: text.into(),
                        start_char: 0,
                        end_char: text.len(),
                    },
                    &emb,
                )
                .unwrap();
        }

        let deleted = store
            .delete_session(&account_a, "shared-session-id")
            .unwrap();
        assert_eq!(deleted, 2);

        assert!(store.query(&account_a, &emb, 10, None).unwrap().is_empty());
        assert!(store
            .query(&account_a_other, &emb, 10, None)
            .unwrap()
            .is_empty());
        let account_b_hits = store.query(&account_b, &emb, 10, None).unwrap();
        assert_eq!(account_b_hits.len(), 1);
        assert_eq!(account_b_hits[0].chunk_text, "keep b");

        let emb_count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM rag_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(emb_count, 1);
    }

    #[test]
    fn vector_store_dim_mismatch_rejected() {
        let store = mem_store(4);
        let scope = scope("account-a", None);
        let chunk = Chunk {
            text: "x".into(),
            start_char: 0,
            end_char: 1,
        };
        let wrong_dim = vec![1.0, 2.0];
        assert!(store.index(&scope, "s", &chunk, &wrong_dim).is_err());
    }

    #[test]
    fn migration_quarantines_then_explicitly_claims_legacy_rows() {
        let base = std::env::temp_dir().join(format!(
            "bluey-rag-migration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("rag_vectors.db");
        let legacy_embedding = vec![1.0, 0.0, 0.0, 0.0];

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE rag_chunks (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     session_id TEXT NOT NULL,
                     text TEXT NOT NULL,
                     start_char INTEGER NOT NULL,
                     end_char INTEGER NOT NULL,
                     ts_ms INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX idx_rag_chunks_session ON rag_chunks(session_id);
                 CREATE TABLE rag_embeddings (
                     chunk_id INTEGER PRIMARY KEY REFERENCES rag_chunks(id) ON DELETE CASCADE,
                     embedding BLOB NOT NULL
                 );
                 INSERT INTO rag_chunks
                     (session_id, text, start_char, end_char, ts_ms)
                 VALUES ('legacy-session', 'legacy account a memory', 0, 23, 0);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO rag_embeddings (chunk_id, embedding) VALUES (1, ?1)",
                params![embedding_to_blob(&legacy_embedding)],
            )
            .unwrap();
        }

        let account_a = scope("account-a", Some("workspace-main"));
        let account_b = scope("account-b", Some("workspace-main"));
        let store = VectorStore::open(&path, 4).unwrap();

        assert!(store
            .query(&account_a, &legacy_embedding, 10, None)
            .unwrap()
            .is_empty());
        assert!(store
            .query(&account_b, &legacy_embedding, 10, None)
            .unwrap()
            .is_empty());

        assert_eq!(
            store
                .claim_legacy_session(&account_a, "legacy-session")
                .unwrap(),
            1
        );
        let account_a_hits = store
            .query(&account_a, &legacy_embedding, 10, None)
            .unwrap();
        assert_eq!(account_a_hits.len(), 1);
        assert_eq!(account_a_hits[0].chunk_text, "legacy account a memory");
        assert!(store
            .query(&account_b, &legacy_embedding, 10, None)
            .unwrap()
            .is_empty());

        drop(store);
        let reopened = VectorStore::open(&path, 4).unwrap();
        assert_eq!(reopened.chunk_count(&account_a).unwrap(), 1);
        drop(reopened);
        std::fs::remove_dir_all(base).unwrap();
    }
}
