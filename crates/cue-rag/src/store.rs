//! Vector store: SQLite-backed chunk storage + in-memory cosine similarity search.
//!
//! v0.1 fallback: embeddings stored as BLOB in SQLite, cosine similarity computed
//! in Rust. Follow-up: swap to sqlite-vec virtual tables for native ANN search.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

const LEGACY_UNSCOPED_ACCOUNT_ID: &str = "__bluey_legacy_unscoped__";
const LEGACY_DIMENSION_ONLY_MODEL: &str = "__bluey_legacy_dimension_only__";
const ACTIVE_EMBEDDING_STATE: &str = "active";
const QUARANTINED_EMBEDDING_STATE: &str = "quarantined";
const MAX_EMBEDDING_MODEL_CHARS: usize = 256;
const EMBEDDING_VALUE_BYTES: usize = std::mem::size_of::<f32>();
pub const RAG_INDEX_SCHEMA_VERSION: u32 = 1;
pub const RAG_STAGING_SESSION_PREFIX: &str = "__bluey_rag_staging__:";

/// Persisted compatibility contract for every active vector in the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RagIndexMetadata {
    pub schema_version: u32,
    pub embedding_model: String,
    pub embedding_dim: usize,
    pub index_generation: u64,
}

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
    metadata: RagIndexMetadata,
}

impl VectorStore {
    /// Open a dimension-only store for backwards compatibility.
    ///
    /// New production callers should use [`Self::open_with_model`] or
    /// [`Self::open_for_provider`] so vectors from different embedding models
    /// can never share an active index generation.
    pub fn open(path: &Path, dim: usize) -> Result<Self> {
        Self::open_with_model(path, LEGACY_DIMENSION_ONLY_MODEL, dim)
    }

    /// Open or create a store for an explicit embedding model and dimension.
    pub fn open_with_model(path: &Path, model: &str, dim: usize) -> Result<Self> {
        let model = validate_embedding_profile(model, dim)?;
        let mut conn = if path.as_os_str() == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(path)?
        };
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let metadata = Self::run_migrations(&mut conn, model, dim)?;
        Ok(Self { conn, metadata })
    }

    /// Open a store using the provider's stable model identity and dimension.
    pub fn open_for_provider(
        path: &Path,
        provider: &dyn crate::embedder::EmbeddingProvider,
    ) -> Result<Self> {
        Self::open_with_model(path, provider.model(), provider.dim())
    }

    /// Active persisted index compatibility metadata.
    pub fn metadata(&self) -> &RagIndexMetadata {
        &self.metadata
    }

    /// Delete an owner-scoped session without opening or changing the active
    /// embedding profile. This is used when no embedder is available during a
    /// local deletion request.
    pub fn delete_session_at_path(
        path: &Path,
        scope: &RagScope,
        session_id: &str,
    ) -> Result<usize> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        if !table_has_column(&conn, "rag_chunks", "session_id")? {
            return Ok(0);
        }
        if table_has_column(&conn, "rag_chunks", "account_id")? {
            return Ok(conn.execute(
                "DELETE FROM rag_chunks WHERE account_id = ?1 AND session_id = ?2",
                params![scope.account_id(), session_id],
            )?);
        }

        // Pre-tenant stores contain only local rows. The daemon verifies the
        // saved meeting owner before invoking this maintenance path.
        Ok(conn.execute(
            "DELETE FROM rag_chunks WHERE session_id = ?1",
            params![session_id],
        )?)
    }

    /// Delete every vector and chunk owned by one account, across all
    /// workspaces and sessions, without requiring an active embedder.
    ///
    /// Account deletion cannot derive its deletion set from MeetingStore:
    /// interrupted imports and old workspace moves may leave valid owner rows
    /// whose parent session is no longer present there. The owner column in
    /// the vector database is the deletion authority for this sweep.
    pub fn delete_account_at_path(path: &Path, account_id: &str) -> Result<usize> {
        let scope = RagScope::new(account_id, None)?;
        if !path.exists() {
            return Ok(0);
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        if !table_has_column(&conn, "rag_chunks", "account_id")? {
            // A pre-tenant index has no evidence tying rows to this owner.
            // Leave those inaccessible legacy rows untouched rather than
            // guessing across an account boundary.
            return Ok(0);
        }
        let tx = conn.transaction()?;
        let deleted = tx.execute(
            "DELETE FROM rag_chunks WHERE account_id = ?1",
            params![scope.account_id()],
        )?;
        let remaining: i64 = tx.query_row(
            "SELECT COUNT(*) FROM rag_chunks WHERE account_id = ?1",
            params![scope.account_id()],
            |row| row.get(0),
        )?;
        anyhow::ensure!(remaining == 0, "RAG account deletion verification failed");
        tx.commit()?;
        Ok(deleted)
    }

    /// Count every chunk owned by one account without opening an embedding
    /// model. Used only to verify durable owner-wide deletion.
    pub fn account_chunk_count_at_path(path: &Path, account_id: &str) -> Result<usize> {
        let scope = RagScope::new(account_id, None)?;
        if !path.exists() {
            return Ok(0);
        }
        let conn = Connection::open(path)?;
        if !table_has_column(&conn, "rag_chunks", "account_id")? {
            return Ok(0);
        }
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM rag_chunks WHERE account_id = ?1",
            params![scope.account_id()],
            |row| row.get(0),
        )?;
        usize::try_from(count).context("RAG account chunk count is outside usize")
    }

    fn run_migrations(
        conn: &mut Connection,
        expected_model: &str,
        expected_dim: usize,
    ) -> Result<RagIndexMetadata> {
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
                 embedding BLOB NOT NULL,
                 embedding_model TEXT NOT NULL DEFAULT '__bluey_legacy_dimension_only__',
                 embedding_dim INTEGER NOT NULL DEFAULT 0,
                 index_generation INTEGER NOT NULL DEFAULT 0,
                 state TEXT NOT NULL DEFAULT 'quarantined',
                 quarantine_reason TEXT
             );
             CREATE TABLE IF NOT EXISTS rag_index_metadata (
                 singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                 schema_version INTEGER NOT NULL,
                 embedding_model TEXT NOT NULL,
                 embedding_dim INTEGER NOT NULL,
                 index_generation INTEGER NOT NULL,
                 updated_ts_ms INTEGER NOT NULL
             );",
        )?;

        if !table_has_column(&tx, "rag_embeddings", "embedding_model")? {
            tx.execute_batch(
                "ALTER TABLE rag_embeddings
                 ADD COLUMN embedding_model TEXT NOT NULL
                 DEFAULT '__bluey_legacy_dimension_only__';",
            )?;
        }
        if !table_has_column(&tx, "rag_embeddings", "embedding_dim")? {
            tx.execute_batch(
                "ALTER TABLE rag_embeddings
                 ADD COLUMN embedding_dim INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !table_has_column(&tx, "rag_embeddings", "index_generation")? {
            tx.execute_batch(
                "ALTER TABLE rag_embeddings
                 ADD COLUMN index_generation INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !table_has_column(&tx, "rag_embeddings", "state")? {
            tx.execute_batch(
                "ALTER TABLE rag_embeddings
                 ADD COLUMN state TEXT NOT NULL DEFAULT 'quarantined';",
            )?;
        }
        if !table_has_column(&tx, "rag_embeddings", "quarantine_reason")? {
            tx.execute_batch("ALTER TABLE rag_embeddings ADD COLUMN quarantine_reason TEXT;")?;
        }

        let expected_dim_i64 = i64::try_from(expected_dim)
            .context("embedding dimension does not fit SQLite metadata")?;
        let expected_blob_bytes = expected_dim_i64
            .checked_mul(4)
            .context("embedding byte dimension overflow")?;
        let stored = read_index_metadata(&tx)?;
        let metadata = match stored {
            None => {
                let metadata = RagIndexMetadata {
                    schema_version: RAG_INDEX_SCHEMA_VERSION,
                    embedding_model: expected_model.to_string(),
                    embedding_dim: expected_dim,
                    index_generation: 1,
                };
                write_index_metadata(&tx, &metadata)?;

                if expected_model == LEGACY_DIMENSION_ONLY_MODEL {
                    tx.execute(
                        "UPDATE rag_embeddings
                         SET embedding_model = ?1,
                             embedding_dim = CASE
                                 WHEN length(embedding) % 4 = 0
                                 THEN length(embedding) / 4
                                 ELSE 0
                             END,
                             index_generation = ?2,
                             state = CASE
                                 WHEN length(embedding) = ?3 THEN ?4
                                 ELSE ?5
                             END,
                             quarantine_reason = CASE
                                 WHEN length(embedding) = ?3 THEN NULL
                                 ELSE 'legacy_dimension_mismatch'
                             END",
                        params![
                            LEGACY_DIMENSION_ONLY_MODEL,
                            i64::try_from(metadata.index_generation)
                                .context("RAG index generation does not fit SQLite")?,
                            expected_blob_bytes,
                            ACTIVE_EMBEDDING_STATE,
                            QUARANTINED_EMBEDDING_STATE,
                        ],
                    )?;
                } else {
                    // A pre-metadata vector's model cannot be proven from its
                    // bytes. Keep it for deletion/audit, but never search it
                    // under a caller-supplied model identity.
                    tx.execute(
                        "UPDATE rag_embeddings
                         SET embedding_model = ?1,
                             embedding_dim = CASE
                                 WHEN length(embedding) % 4 = 0
                                 THEN length(embedding) / 4
                                 ELSE 0
                             END,
                             index_generation = 0,
                             state = ?2,
                             quarantine_reason = 'legacy_model_unknown'",
                        params![LEGACY_DIMENSION_ONLY_MODEL, QUARANTINED_EMBEDDING_STATE],
                    )?;
                }
                metadata
            }
            Some(stored) => {
                anyhow::ensure!(
                    stored.schema_version <= RAG_INDEX_SCHEMA_VERSION,
                    "RAG index schema {} is newer than supported schema {}",
                    stored.schema_version,
                    RAG_INDEX_SCHEMA_VERSION
                );
                if stored.embedding_model != expected_model || stored.embedding_dim != expected_dim
                {
                    let next_generation = stored
                        .index_generation
                        .checked_add(1)
                        .context("RAG index generation overflow")?;
                    tx.execute(
                        "UPDATE rag_embeddings
                         SET state = ?1,
                             quarantine_reason = 'index_profile_changed'
                         WHERE state = ?2",
                        params![QUARANTINED_EMBEDDING_STATE, ACTIVE_EMBEDDING_STATE],
                    )?;
                    let metadata = RagIndexMetadata {
                        schema_version: RAG_INDEX_SCHEMA_VERSION,
                        embedding_model: expected_model.to_string(),
                        embedding_dim: expected_dim,
                        index_generation: next_generation,
                    };
                    write_index_metadata(&tx, &metadata)?;
                    metadata
                } else {
                    let metadata = RagIndexMetadata {
                        schema_version: RAG_INDEX_SCHEMA_VERSION,
                        ..stored
                    };
                    write_index_metadata(&tx, &metadata)?;
                    metadata
                }
            }
        };

        quarantine_incompatible_embeddings(&tx, &metadata)?;
        tx.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_rag_embeddings_compatibility
             ON rag_embeddings(
                 state,
                 embedding_model,
                 embedding_dim,
                 index_generation
             );",
        )?;
        tx.commit().context("failed to commit RAG migrations")?;
        Ok(metadata)
    }

    /// Index a chunk with its embedding.
    pub fn index(
        &self,
        scope: &RagScope,
        session_id: &str,
        chunk: &crate::Chunk,
        embedding: &[f32],
    ) -> Result<()> {
        validate_embedding(embedding, self.metadata.embedding_dim)?;
        let generation = i64::try_from(self.metadata.index_generation)
            .context("RAG index generation does not fit SQLite")?;
        let dimension = i64::try_from(self.metadata.embedding_dim)
            .context("embedding dimension does not fit SQLite")?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
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
        let chunk_id = tx.last_insert_rowid();
        let blob = embedding_to_blob(embedding);
        tx.execute(
            "INSERT INTO rag_embeddings (
                 chunk_id,
                 embedding,
                 embedding_model,
                 embedding_dim,
                 index_generation,
                 state,
                 quarantine_reason
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
            params![
                chunk_id,
                blob,
                self.metadata.embedding_model,
                dimension,
                generation,
                ACTIVE_EMBEDDING_STATE,
            ],
        )?;
        tx.commit()?;
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
        validate_embedding(query_embedding, self.metadata.embedding_dim)
            .context("invalid query embedding")?;

        if limit == 0 {
            return Ok(Vec::new());
        }
        let dimension = i64::try_from(self.metadata.embedding_dim)
            .context("embedding dimension does not fit SQLite")?;
        let generation = i64::try_from(self.metadata.index_generation)
            .context("RAG index generation does not fit SQLite")?;

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
        let mut invalid_blob_ids = Vec::new();

        let mut push_row = |embedding_id: i64, sid: String, text: String, blob: Vec<u8>| {
            let emb = match blob_to_embedding(&blob, self.metadata.embedding_dim) {
                Ok(embedding) => embedding,
                Err(_) => {
                    invalid_blob_ids.push(embedding_id);
                    return;
                }
            };
            let score = match cosine_similarity(query_embedding, &emb) {
                Ok(score) => score,
                Err(_) => {
                    invalid_blob_ids.push(embedding_id);
                    return;
                }
            };
            heap.push(Reverse((OrdF32(score), sid, text)));
            if heap.len() > limit {
                heap.pop();
            }
        };

        match session_id {
            Some(sid) => {
                let mut stmt = self.conn.prepare(
                    "SELECT e.chunk_id, c.session_id, c.text, e.embedding
                     FROM rag_chunks c
                     JOIN rag_embeddings e ON e.chunk_id = c.id
                     WHERE c.account_id = ?1
                       AND c.workspace_id IS ?2
                       AND c.session_id = ?3
                       AND e.state = ?4
                       AND e.embedding_model = ?5
                       AND e.embedding_dim = ?6
                       AND e.index_generation = ?7",
                )?;
                let mut rows = stmt.query(params![
                    scope.account_id(),
                    scope.workspace_id(),
                    sid,
                    ACTIVE_EMBEDDING_STATE,
                    self.metadata.embedding_model,
                    dimension,
                    generation,
                ])?;
                while let Some(row) = rows.next()? {
                    push_row(row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
                }
            }
            None => {
                let mut stmt = self.conn.prepare(
                    "SELECT e.chunk_id, c.session_id, c.text, e.embedding
                     FROM rag_chunks c
                     JOIN rag_embeddings e ON e.chunk_id = c.id
                     WHERE c.account_id = ?1
                       AND c.workspace_id IS ?2
                       AND substr(c.session_id, 1, ?3) != ?4
                       AND e.state = ?5
                       AND e.embedding_model = ?6
                       AND e.embedding_dim = ?7
                       AND e.index_generation = ?8",
                )?;
                let mut rows = stmt.query(params![
                    scope.account_id(),
                    scope.workspace_id(),
                    RAG_STAGING_SESSION_PREFIX.len(),
                    RAG_STAGING_SESSION_PREFIX,
                    ACTIVE_EMBEDDING_STATE,
                    self.metadata.embedding_model,
                    dimension,
                    generation,
                ])?;
                while let Some(row) = rows.next()? {
                    push_row(row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
                }
            }
        }
        quarantine_embedding_ids(&self.conn, &invalid_blob_ids, "invalid_embedding_blob")?;

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

    /// Atomically replaces a visible session with a fully built staging
    /// generation. Global queries exclude staging rows, so readers see either
    /// the old complete index or the new complete index.
    pub fn replace_session_from_staging(
        &self,
        scope: &RagScope,
        session_id: &str,
        staging_session_id: &str,
    ) -> Result<usize> {
        anyhow::ensure!(
            staging_session_id.starts_with(RAG_STAGING_SESSION_PREFIX),
            "invalid RAG staging session id"
        );
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM rag_chunks WHERE account_id = ?1 AND session_id = ?2",
            params![scope.account_id(), session_id],
        )?;
        let promoted = tx.execute(
            "UPDATE rag_chunks
             SET session_id = ?1
             WHERE account_id = ?2
               AND workspace_id IS ?3
               AND session_id = ?4",
            params![
                session_id,
                scope.account_id(),
                scope.workspace_id(),
                staging_session_id
            ],
        )?;
        tx.commit()?;
        Ok(promoted)
    }

    /// Removes abandoned staging generations for this owner/workspace.
    pub fn delete_staging_sessions(&self, scope: &RagScope) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM rag_chunks
             WHERE account_id = ?1
               AND workspace_id IS ?2
               AND substr(session_id, 1, ?3) = ?4",
            params![
                scope.account_id(),
                scope.workspace_id(),
                RAG_STAGING_SESSION_PREFIX.len(),
                RAG_STAGING_SESSION_PREFIX
            ],
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
        let dimension = i64::try_from(self.metadata.embedding_dim)
            .context("embedding dimension does not fit SQLite")?;
        let generation = i64::try_from(self.metadata.index_generation)
            .context("RAG index generation does not fit SQLite")?;
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM rag_chunks
             JOIN rag_embeddings e ON e.chunk_id = rag_chunks.id
             WHERE account_id = ?1
               AND workspace_id IS ?2
               AND substr(session_id, 1, ?3) != ?4
               AND e.state = ?5
               AND e.embedding_model = ?6
               AND e.embedding_dim = ?7
               AND e.index_generation = ?8",
            params![
                scope.account_id(),
                scope.workspace_id(),
                RAG_STAGING_SESSION_PREFIX.len(),
                RAG_STAGING_SESSION_PREFIX,
                ACTIVE_EMBEDDING_STATE,
                self.metadata.embedding_model,
                dimension,
                generation,
            ],
            |r| r.get(0),
        )?;
        usize::try_from(count).context("RAG chunk count is outside usize")
    }

    /// Number of retained vectors excluded from search pending reindex/delete.
    pub fn quarantined_embedding_count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM rag_embeddings WHERE state = ?1",
            params![QUARANTINED_EMBEDDING_STATE],
            |row| row.get(0),
        )?;
        usize::try_from(count).context("quarantined embedding count is outside usize")
    }
}

fn validate_embedding_profile(model: &str, dim: usize) -> Result<&str> {
    let model = model.trim();
    anyhow::ensure!(!model.is_empty(), "embedding model cannot be empty");
    anyhow::ensure!(
        model.chars().count() <= MAX_EMBEDDING_MODEL_CHARS,
        "embedding model exceeds {MAX_EMBEDDING_MODEL_CHARS} characters"
    );
    anyhow::ensure!(dim > 0, "embedding dimension must be positive");
    let _ = dim
        .checked_mul(EMBEDDING_VALUE_BYTES)
        .context("embedding dimension byte size overflow")?;
    Ok(model)
}

fn read_index_metadata(conn: &Connection) -> Result<Option<RagIndexMetadata>> {
    let stored: Option<(i64, String, i64, i64)> = conn
        .query_row(
            "SELECT schema_version, embedding_model, embedding_dim, index_generation
             FROM rag_index_metadata
             WHERE singleton_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((schema_version, embedding_model, embedding_dim, index_generation)) = stored else {
        return Ok(None);
    };
    anyhow::ensure!(schema_version > 0, "invalid RAG index schema version");
    anyhow::ensure!(
        embedding_dim > 0,
        "invalid persisted RAG embedding dimension"
    );
    anyhow::ensure!(
        index_generation > 0,
        "invalid persisted RAG index generation"
    );
    validate_embedding_profile(
        &embedding_model,
        usize::try_from(embedding_dim)
            .context("persisted RAG embedding dimension is outside usize")?,
    )?;
    Ok(Some(RagIndexMetadata {
        schema_version: u32::try_from(schema_version)
            .context("persisted RAG schema version is outside u32")?,
        embedding_model,
        embedding_dim: usize::try_from(embedding_dim)
            .context("persisted RAG embedding dimension is outside usize")?,
        index_generation: u64::try_from(index_generation)
            .context("persisted RAG index generation is outside u64")?,
    }))
}

fn write_index_metadata(conn: &Connection, metadata: &RagIndexMetadata) -> Result<()> {
    conn.execute(
        "INSERT INTO rag_index_metadata (
             singleton_id,
             schema_version,
             embedding_model,
             embedding_dim,
             index_generation,
             updated_ts_ms
         )
         VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(singleton_id) DO UPDATE SET
             schema_version = excluded.schema_version,
             embedding_model = excluded.embedding_model,
             embedding_dim = excluded.embedding_dim,
             index_generation = excluded.index_generation,
             updated_ts_ms = excluded.updated_ts_ms",
        params![
            i64::from(metadata.schema_version),
            metadata.embedding_model,
            i64::try_from(metadata.embedding_dim)
                .context("embedding dimension does not fit SQLite")?,
            i64::try_from(metadata.index_generation)
                .context("RAG index generation does not fit SQLite")?,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn quarantine_incompatible_embeddings(
    conn: &Connection,
    metadata: &RagIndexMetadata,
) -> Result<usize> {
    let dimension =
        i64::try_from(metadata.embedding_dim).context("embedding dimension does not fit SQLite")?;
    let generation = i64::try_from(metadata.index_generation)
        .context("RAG index generation does not fit SQLite")?;
    let expected_bytes = dimension
        .checked_mul(
            i64::try_from(EMBEDDING_VALUE_BYTES)
                .context("embedding value size does not fit SQLite")?,
        )
        .context("embedding byte dimension overflow")?;
    let updated = conn.execute(
        "UPDATE rag_embeddings
         SET state = ?1,
             quarantine_reason = CASE
                 WHEN embedding_model != ?2 THEN 'embedding_model_mismatch'
                 WHEN embedding_dim != ?3 THEN 'embedding_dimension_mismatch'
                 WHEN index_generation != ?4 THEN 'index_generation_mismatch'
                 ELSE 'embedding_blob_dimension_mismatch'
             END
         WHERE state = ?5
           AND (
               embedding_model != ?2
               OR embedding_dim != ?3
               OR index_generation != ?4
               OR length(embedding) != ?6
           )",
        params![
            QUARANTINED_EMBEDDING_STATE,
            metadata.embedding_model,
            dimension,
            generation,
            ACTIVE_EMBEDDING_STATE,
            expected_bytes,
        ],
    )?;
    Ok(updated)
}

fn quarantine_embedding_ids(conn: &Connection, ids: &[i64], reason: &str) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for id in ids {
        tx.execute(
            "UPDATE rag_embeddings
             SET state = ?1, quarantine_reason = ?2
             WHERE chunk_id = ?3 AND state = ?4",
            params![
                QUARANTINED_EMBEDDING_STATE,
                reason,
                id,
                ACTIVE_EMBEDDING_STATE
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
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

fn validate_embedding(embedding: &[f32], expected_dim: usize) -> Result<()> {
    anyhow::ensure!(
        embedding.len() == expected_dim,
        "embedding dim mismatch: expected {}, got {}",
        expected_dim,
        embedding.len()
    );
    anyhow::ensure!(
        embedding.iter().all(|value| value.is_finite()),
        "embedding contains a non-finite value"
    );
    Ok(())
}

fn blob_to_embedding(blob: &[u8], expected_dim: usize) -> Result<Vec<f32>> {
    let expected_bytes = expected_dim
        .checked_mul(EMBEDDING_VALUE_BYTES)
        .context("embedding byte dimension overflow")?;
    anyhow::ensure!(
        blob.len() == expected_bytes,
        "embedding blob dim mismatch: expected {} bytes, got {}",
        expected_bytes,
        blob.len()
    );
    let mut embedding = Vec::with_capacity(expected_dim);
    let (values, remainder) = blob.as_chunks::<EMBEDDING_VALUE_BYTES>();
    debug_assert!(remainder.is_empty());
    for bytes in values {
        let value = f32::from_le_bytes(*bytes);
        anyhow::ensure!(
            value.is_finite(),
            "embedding blob contains a non-finite value"
        );
        embedding.push(value);
    }
    Ok(embedding)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32> {
    anyhow::ensure!(
        a.len() == b.len(),
        "cosine similarity dimension mismatch: {} != {}",
        a.len(),
        b.len()
    );
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for index in 0..a.len() {
        let ai = a[index];
        let bi = b[index];
        dot += ai * bi;
        norm_a += ai * ai;
        norm_b += bi * bi;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        Ok(0.0)
    } else {
        Ok(dot / denom)
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

    fn temp_database(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "bluey-rag-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join("rag_vectors.db");
        (base, path)
    }

    fn test_chunk(text: &str) -> Chunk {
        Chunk {
            text: text.to_string(),
            start_char: 0,
            end_char: text.len(),
        }
    }

    fn create_legacy_database(path: &Path, embedding: &[f32]) {
        let conn = Connection::open(path).unwrap();
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
             VALUES ('legacy-session', 'legacy memory', 0, 13, 0);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO rag_embeddings (chunk_id, embedding) VALUES (1, ?1)",
            params![embedding_to_blob(embedding)],
        )
        .unwrap();
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
    fn staged_session_is_hidden_until_atomic_promotion() {
        let store = mem_store(4);
        let scope = scope("account-a", Some("workspace-main"));
        let emb = vec![1.0, 0.0, 0.0, 0.0];
        store
            .index(
                &scope,
                "session-a",
                &Chunk {
                    text: "old complete memory".into(),
                    start_char: 0,
                    end_char: 19,
                },
                &emb,
            )
            .unwrap();
        let staging = format!("{RAG_STAGING_SESSION_PREFIX}session-a:test");
        store
            .index(
                &scope,
                &staging,
                &Chunk {
                    text: "new complete memory".into(),
                    start_char: 0,
                    end_char: 19,
                },
                &emb,
            )
            .unwrap();

        let before = store.query(&scope, &emb, 10, None).unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].chunk_text, "old complete memory");

        assert_eq!(
            store
                .replace_session_from_staging(&scope, "session-a", &staging)
                .unwrap(),
            1
        );
        let after = store.query(&scope, &emb, 10, None).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].session_id, "session-a");
        assert_eq!(after[0].chunk_text, "new complete memory");
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
        assert!(store
            .query(&scope, &wrong_dim, 10, None)
            .unwrap_err()
            .to_string()
            .contains("invalid query embedding"));
        assert!(store
            .index(&scope, "s", &chunk, &[f32::NAN, 0.0, 0.0, 0.0])
            .is_err());
        assert!(cosine_similarity(&[1.0, 0.0], &[1.0]).is_err());
    }

    #[test]
    fn index_metadata_persists_without_generation_churn() {
        let (base, path) = temp_database("metadata");
        let owner = scope("account-a", None);
        let embedding = vec![1.0, 0.0, 0.0, 0.0];
        {
            let store = VectorStore::open_with_model(&path, "model-a", 4).unwrap();
            assert_eq!(
                store.metadata(),
                &RagIndexMetadata {
                    schema_version: RAG_INDEX_SCHEMA_VERSION,
                    embedding_model: "model-a".to_string(),
                    embedding_dim: 4,
                    index_generation: 1,
                }
            );
            store
                .index(&owner, "session-a", &test_chunk("persisted"), &embedding)
                .unwrap();
        }

        let reopened = VectorStore::open_with_model(&path, "model-a", 4).unwrap();
        assert_eq!(reopened.metadata().index_generation, 1);
        assert_eq!(
            reopened.query(&owner, &embedding, 10, None).unwrap()[0].chunk_text,
            "persisted"
        );
        drop(reopened);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn model_change_quarantines_old_generation_and_delete_cascades() {
        let (base, path) = temp_database("model-change");
        let owner = scope("account-a", Some("workspace-main"));
        let old_embedding = vec![1.0, 0.0, 0.0, 0.0];
        {
            let store = VectorStore::open_with_model(&path, "model-a", 4).unwrap();
            store
                .index(
                    &owner,
                    "old-session",
                    &test_chunk("old model vector"),
                    &old_embedding,
                )
                .unwrap();
        }

        let new_embedding = vec![0.0, 1.0, 0.0, 0.0];
        let store = VectorStore::open_with_model(&path, "model-b", 4).unwrap();
        assert_eq!(store.metadata().index_generation, 2);
        assert_eq!(store.metadata().embedding_model, "model-b");
        assert_eq!(store.quarantined_embedding_count().unwrap(), 1);
        assert!(store
            .query(&owner, &old_embedding, 10, None)
            .unwrap()
            .is_empty());

        store
            .index(
                &owner,
                "new-session",
                &test_chunk("new model vector"),
                &new_embedding,
            )
            .unwrap();
        let hits = store.query(&owner, &new_embedding, 10, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_text, "new model vector");

        assert_eq!(store.delete_session(&owner, "old-session").unwrap(), 1);
        assert_eq!(store.quarantined_embedding_count().unwrap(), 0);
        assert_eq!(store.chunk_count(&owner).unwrap(), 1);
        drop(store);

        let reopened = VectorStore::open_with_model(&path, "model-b", 4).unwrap();
        assert_eq!(reopened.metadata().index_generation, 2);
        assert_eq!(
            reopened
                .query(&owner, &new_embedding, 10, None)
                .unwrap()
                .len(),
            1
        );
        drop(reopened);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn offline_delete_preserves_active_embedding_profile() {
        let (base, path) = temp_database("offline-delete");
        let owner = scope("account-a", None);
        {
            let store = VectorStore::open_with_model(&path, "model-a", 4).unwrap();
            store
                .index(
                    &owner,
                    "delete-me",
                    &test_chunk("remove this"),
                    &[1.0, 0.0, 0.0, 0.0],
                )
                .unwrap();
            store
                .index(
                    &owner,
                    "keep-me",
                    &test_chunk("keep this"),
                    &[0.0, 1.0, 0.0, 0.0],
                )
                .unwrap();
        }

        assert_eq!(
            VectorStore::delete_session_at_path(&path, &owner, "delete-me").unwrap(),
            1
        );
        let reopened = VectorStore::open_with_model(&path, "model-a", 4).unwrap();
        assert_eq!(reopened.metadata().index_generation, 1);
        assert_eq!(reopened.metadata().embedding_model, "model-a");
        assert_eq!(reopened.chunk_count(&owner).unwrap(), 1);
        assert_eq!(
            reopened
                .query(&owner, &[0.0, 1.0, 0.0, 0.0], 10, None)
                .unwrap()[0]
                .session_id,
            "keep-me"
        );
        drop(reopened);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn dimension_change_quarantines_old_vectors_before_search() {
        let (base, path) = temp_database("dimension-change");
        let owner = scope("account-a", None);
        {
            let store = VectorStore::open_with_model(&path, "model-a", 4).unwrap();
            store
                .index(
                    &owner,
                    "old-session",
                    &test_chunk("four dimensions"),
                    &[1.0, 0.0, 0.0, 0.0],
                )
                .unwrap();
        }

        let store = VectorStore::open_with_model(&path, "model-a", 3).unwrap();
        assert_eq!(store.metadata().embedding_dim, 3);
        assert_eq!(store.metadata().index_generation, 2);
        assert_eq!(store.quarantined_embedding_count().unwrap(), 1);
        assert!(store
            .query(&owner, &[1.0, 0.0, 0.0], 10, None)
            .unwrap()
            .is_empty());
        store
            .index(
                &owner,
                "new-session",
                &test_chunk("three dimensions"),
                &[1.0, 0.0, 0.0],
            )
            .unwrap();
        assert_eq!(
            store.query(&owner, &[1.0, 0.0, 0.0], 10, None).unwrap()[0].chunk_text,
            "three dimensions"
        );
        drop(store);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn known_model_quarantines_unverifiable_legacy_vectors() {
        let (base, path) = temp_database("legacy-model");
        let embedding = vec![1.0, 0.0, 0.0, 0.0];
        create_legacy_database(&path, &embedding);

        let owner = scope("account-a", None);
        let store = VectorStore::open_with_model(&path, "known-model", 4).unwrap();
        assert_eq!(
            store
                .claim_legacy_session(&owner, "legacy-session")
                .unwrap(),
            1
        );
        assert_eq!(store.quarantined_embedding_count().unwrap(), 1);
        assert!(store
            .query(&owner, &embedding, 10, None)
            .unwrap()
            .is_empty());
        assert_eq!(store.chunk_count(&owner).unwrap(), 0);
        assert_eq!(store.delete_session(&owner, "legacy-session").unwrap(), 1);
        assert_eq!(store.quarantined_embedding_count().unwrap(), 0);
        drop(store);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn legacy_dimension_mismatch_is_quarantined_not_truncated() {
        let (base, path) = temp_database("legacy-dimension");
        create_legacy_database(&path, &[1.0, 0.0]);

        let owner = scope("account-a", None);
        let store = VectorStore::open(&path, 4).unwrap();
        assert_eq!(
            store
                .claim_legacy_session(&owner, "legacy-session")
                .unwrap(),
            1
        );
        assert_eq!(store.quarantined_embedding_count().unwrap(), 1);
        assert!(store
            .query(&owner, &[1.0, 0.0, 0.0, 0.0], 10, None)
            .unwrap()
            .is_empty());
        drop(store);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn query_quarantines_corrupt_active_blob_instead_of_zipping() {
        let store = VectorStore::open_with_model(Path::new(":memory:"), "model-a", 4).unwrap();
        let owner = scope("account-a", None);
        store
            .index(
                &owner,
                "session-a",
                &test_chunk("corrupt me"),
                &[1.0, 0.0, 0.0, 0.0],
            )
            .unwrap();
        store
            .conn
            .execute(
                "UPDATE rag_embeddings SET embedding = ?1",
                params![embedding_to_blob(&[1.0, 0.0])],
            )
            .unwrap();

        assert!(store
            .query(&owner, &[1.0, 0.0, 0.0, 0.0], 10, None)
            .unwrap()
            .is_empty());
        assert_eq!(store.quarantined_embedding_count().unwrap(), 1);
        let reason: String = store
            .conn
            .query_row("SELECT quarantine_reason FROM rag_embeddings", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(reason, "invalid_embedding_blob");
    }

    #[test]
    fn future_index_schema_is_rejected_without_downgrade() {
        let (base, path) = temp_database("future-schema");
        drop(VectorStore::open_with_model(&path, "model-a", 4).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE rag_index_metadata SET schema_version = ?1",
            params![i64::from(RAG_INDEX_SCHEMA_VERSION) + 1],
        )
        .unwrap();
        drop(conn);

        let error = VectorStore::open_with_model(&path, "model-a", 4)
            .err()
            .expect("future schema must be rejected");
        assert!(error.to_string().contains("newer than supported"));
        let persisted: i64 = Connection::open(&path)
            .unwrap()
            .query_row("SELECT schema_version FROM rag_index_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(persisted, i64::from(RAG_INDEX_SCHEMA_VERSION) + 1);
        std::fs::remove_dir_all(base).unwrap();
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

    #[test]
    fn owner_wide_delete_spans_workspaces_and_preserves_other_accounts() {
        let (base, path) = temp_database("delete-account");
        let store = VectorStore::open(&path, 4).unwrap();
        let owner_a_one = scope("account-a", Some("workspace-one"));
        let owner_a_two = scope("account-a", Some("workspace-two"));
        let owner_b = scope("account-b", Some("workspace-one"));
        let embedding = fake_embedding(4, 0.25);
        store
            .index(
                &owner_a_one,
                "known-session",
                &test_chunk("known owner A memory"),
                &embedding,
            )
            .unwrap();
        store
            .index(
                &owner_a_two,
                "orphan-session",
                &test_chunk("orphan owner A memory"),
                &embedding,
            )
            .unwrap();
        store
            .index(
                &owner_b,
                "other-session",
                &test_chunk("owner B memory"),
                &embedding,
            )
            .unwrap();
        drop(store);

        assert_eq!(
            VectorStore::account_chunk_count_at_path(&path, "account-a").unwrap(),
            2
        );
        assert_eq!(
            VectorStore::delete_account_at_path(&path, "account-a").unwrap(),
            2
        );
        assert_eq!(
            VectorStore::account_chunk_count_at_path(&path, "account-a").unwrap(),
            0
        );
        assert_eq!(
            VectorStore::account_chunk_count_at_path(&path, "account-b").unwrap(),
            1
        );

        let reopened = VectorStore::open(&path, 4).unwrap();
        assert!(reopened
            .query(&owner_a_one, &embedding, 10, None)
            .unwrap()
            .is_empty());
        assert_eq!(
            reopened
                .query(&owner_b, &embedding, 10, None)
                .unwrap()
                .len(),
            1
        );
        drop(reopened);
        std::fs::remove_dir_all(base).unwrap();
    }
}
