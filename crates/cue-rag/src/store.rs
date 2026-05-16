//! Vector store: SQLite-backed chunk storage + in-memory cosine similarity search.
//!
//! v0.1 fallback: embeddings stored as BLOB in SQLite, cosine similarity computed
//! in Rust. Follow-up: swap to sqlite-vec virtual tables for native ANN search.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// A search result from the vector store.
#[derive(Debug, Clone)]
pub struct RagHit {
    pub session_id: String,
    pub chunk_text: String,
    pub score: f32,
}

/// Vector store backed by SQLite with in-memory cosine similarity.
pub struct VectorStore {
    conn: Connection,
    dim: usize,
}

impl VectorStore {
    /// Open or create the vector store database.
    pub fn open(path: &Path, dim: usize) -> Result<Self> {
        let conn = if path.as_os_str() == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(path)?
        };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Self::run_migrations(&conn)?;
        Ok(Self { conn, dim })
    }

    fn run_migrations(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rag_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                text TEXT NOT NULL,
                start_char INTEGER NOT NULL,
                end_char INTEGER NOT NULL,
                ts_ms INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_rag_chunks_session ON rag_chunks(session_id);
            CREATE TABLE IF NOT EXISTS rag_embeddings (
                chunk_id INTEGER PRIMARY KEY REFERENCES rag_chunks(id) ON DELETE CASCADE,
                embedding BLOB NOT NULL
            );"
        ).context("failed to run RAG migrations")?;
        Ok(())
    }

    /// Index a chunk with its embedding.
    pub fn index(
        &self,
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
            "INSERT INTO rag_chunks (session_id, text, start_char, end_char, ts_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, chunk.text, chunk.start_char, chunk.end_char, now_ms()],
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
    pub fn query(
        &self,
        query_embedding: &[f32],
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<RagHit>> {
        anyhow::ensure!(
            query_embedding.len() == self.dim,
            "query embedding dim mismatch"
        );

        let mut scored: Vec<RagHit> = Vec::new();

        match session_id {
            Some(sid) => {
                let mut stmt = self.conn.prepare(
                    "SELECT c.session_id, c.text, e.embedding FROM rag_chunks c                      JOIN rag_embeddings e ON e.chunk_id = c.id                      WHERE c.session_id = ?1"
                )?;
                let mut rows = stmt.query(params![sid])?;
                while let Some(row) = rows.next()? {
                    let s: String = row.get(0)?;
                    let text: String = row.get(1)?;
                    let blob: Vec<u8> = row.get(2)?;
                    let emb = blob_to_embedding(&blob);
                    let score = cosine_similarity(query_embedding, &emb);
                    scored.push(RagHit { session_id: s, chunk_text: text, score });
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT c.session_id, c.text, e.embedding FROM rag_chunks c                      JOIN rag_embeddings e ON e.chunk_id = c.id"
                )?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    let s: String = row.get(0)?;
                    let text: String = row.get(1)?;
                    let blob: Vec<u8> = row.get(2)?;
                    let emb = blob_to_embedding(&blob);
                    let score = cosine_similarity(query_embedding, &emb);
                    scored.push(RagHit { session_id: s, chunk_text: text, score });
                }
            }
        }

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    /// Delete all chunks and embeddings for a session.
    pub fn delete_session(&self, session_id: &str) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM rag_chunks WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(deleted)
    }

    /// Number of indexed chunks.
    pub fn chunk_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM rag_chunks", [], |r| r.get(0))?;
        Ok(count as usize)
    }
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
    if denom == 0.0 { 0.0 } else { dot / denom }
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

    fn fake_embedding(dim: usize, seed: f32) -> Vec<f32> {
        (0..dim).map(|i| ((i as f32) * seed).sin()).collect()
    }

    #[test]
    fn vector_store_index_and_query() {
        let dim = 8;
        let store = mem_store(dim);

        for i in 0..5 {
            let chunk = Chunk {
                text: format!("chunk {i}"),
                start_char: i * 10,
                end_char: (i + 1) * 10,
            };
            let emb = fake_embedding(dim, (i + 1) as f32 * 0.3);
            store.index("session-1", &chunk, &emb).unwrap();
        }

        let query = fake_embedding(dim, 0.9); // close to seed=0.9 (i=2, seed=(2+1)*0.3=0.9)
        let results = store.query(&query, 3, None).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].score >= results[1].score);
        assert!(results[1].score >= results[2].score);
        assert_eq!(results[0].chunk_text, "chunk 2");
    }

    #[test]
    fn vector_store_session_filter() {
        let dim = 4;
        let store = mem_store(dim);

        let emb = vec![1.0, 0.0, 0.0, 0.0];
        store.index("s1", &Chunk { text: "s1 chunk".into(), start_char: 0, end_char: 8 }, &emb).unwrap();
        store.index("s2", &Chunk { text: "s2 chunk".into(), start_char: 0, end_char: 8 }, &emb).unwrap();

        let results = store.query(&emb, 10, Some("s1")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "s1");

        let results = store.query(&emb, 10, None).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn vector_store_delete_session_cascade() {
        let dim = 4;
        let store = mem_store(dim);
        let emb = vec![1.0, 0.0, 0.0, 0.0];

        store.index("s1", &Chunk { text: "keep".into(), start_char: 0, end_char: 4 }, &emb).unwrap();
        store.index("s2", &Chunk { text: "delete me".into(), start_char: 0, end_char: 9 }, &emb).unwrap();

        let deleted = store.delete_session("s2").unwrap();
        assert_eq!(deleted, 1);

        let results = store.query(&emb, 10, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "s1");

        let emb_count: i64 = store.conn
            .query_row("SELECT COUNT(*) FROM rag_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(emb_count, 1);
    }

    #[test]
    fn vector_store_dim_mismatch_rejected() {
        let store = mem_store(4);
        let chunk = Chunk { text: "x".into(), start_char: 0, end_char: 1 };
        let wrong_dim = vec![1.0, 2.0];
        assert!(store.index("s", &chunk, &wrong_dim).is_err());
    }
}
