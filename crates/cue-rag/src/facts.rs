//! Cross-meeting facts memory — the long-term tier of the two-tier memory model
//! (PLAN-CONTEXT-WARMUP Appendix A/E, the Mem0/Zep patterns on our stack).
//!
//! Stores EXTRACTED, quote-verified facts (never raw transcript — the measured
//! rule: facts retrieve at ~100% top-3, raw transcript confidently mismatches)
//! with their embeddings in a single SQLite file. Update semantics:
//!
//! - **exact duplicate** (same normalized text hash) → NOOP.
//! - **near-duplicate** (cosine ≥ [`DUP_THRESHOLD`]) → NOOP (already known).
//! - **same-topic revision** (cosine in [`SUPERSEDE_THRESHOLD`]..DUP) →
//!   SUPERSEDE: close the old fact's validity window (`valid_to`), insert the
//!   new one. History stays queryable — reversed decisions are never erased
//!   (the Zep temporal pattern; LongMemEval "knowledge updates" = 100% this way).
//! - otherwise → ADD.
//!
//! Two consolidation paths share this store (PLAN-CONTEXT-WARMUP Appendix E):
//!
//! 1. **Agent-decided (the Mem0 paper's update phase)** — the daemon batches
//!    new facts + the most-similar existing ones into one update-decision
//!    prompt; the returned ADD / UPDATE / DELETE / NONE ops land here via
//!    [`FactsStore::insert_fact`] / [`FactsStore::supersede_fact`] /
//!    [`FactsStore::invalidate_fact`]. (mem0 v3 dropped this phase and went
//!    additive-only — wrong for meetings, where decisions get REVERSED; we
//!    keep the paper loop, verified against mem0 source 2026-07.)
//! 2. **Similarity heuristic** ([`FactsStore::add_fact`]) — the fallback when
//!    no agent is attached or its output is unusable.
//!
//! Every mutating op is recorded in `facts_history` (mem0's audit-log pattern:
//! `memory_id, old_memory, new_memory, event, is_deleted, at`) — NONE is never
//! logged, matching mem0.

use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// Near-duplicate: the fact is already known — do nothing.
pub const DUP_THRESHOLD: f32 = 0.93;
/// Same topic, materially different content — supersede the old fact.
pub const SUPERSEDE_THRESHOLD: f32 = 0.86;

/// What happened when a fact was offered to the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddOutcome {
    Added,
    Duplicate,
    /// The new fact replaced an older same-topic fact (old id returned; the old
    /// row stays, with its validity window closed).
    Superseded {
        previous_id: i64,
    },
}

/// One retrieved fact.
#[derive(Debug, Clone)]
pub struct FactHit {
    pub text: String,
    pub meeting_id: String,
    pub score: f32,
    pub created_at_ms: i64,
}

/// One CURRENT fact with its row id — what the update-decision phase retrieves
/// and presents to the agent (via small display indexes, never raw ids).
#[derive(Debug, Clone)]
pub struct FactRow {
    pub id: i64,
    pub text: String,
    pub score: f32,
}

/// One audit-trail row (mem0 history-table pattern).
#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub memory_id: i64,
    pub old_memory: Option<String>,
    pub new_memory: Option<String>,
    pub event: String,
    pub is_deleted: bool,
    pub at_ms: i64,
}

/// SQLite-backed facts store. `Mutex<Connection>` — passes are short and this
/// is shared across async tasks via `Arc`.
pub struct FactsStore {
    conn: Mutex<Connection>,
    dim: usize,
}

impl FactsStore {
    pub fn open(path: &Path, dim: usize) -> Result<Self> {
        let conn = if path.as_os_str() == ":memory:" {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(path)?
        };
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS facts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL,
                text TEXT NOT NULL,
                hash TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                valid_to INTEGER,
                embedding BLOB NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_facts_hash ON facts(hash);
            CREATE INDEX IF NOT EXISTS idx_facts_valid ON facts(valid_to);
            CREATE TABLE IF NOT EXISTS facts_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id INTEGER NOT NULL,
                old_memory TEXT,
                new_memory TEXT,
                event TEXT NOT NULL,
                is_deleted INTEGER NOT NULL DEFAULT 0,
                at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_facts_history_memory
                ON facts_history(memory_id);",
        )
        .context("failed to run facts migrations")?;
        Ok(Self {
            conn: Mutex::new(conn),
            dim,
        })
    }

    /// Offer one extracted fact. See the module docs for the ADD / NOOP /
    /// SUPERSEDE semantics.
    pub fn add_fact(&self, meeting_id: &str, text: &str, embedding: &[f32]) -> Result<AddOutcome> {
        anyhow::ensure!(embedding.len() == self.dim, "embedding dim mismatch");
        let text = text.trim();
        anyhow::ensure!(!text.is_empty(), "empty fact");
        let hash = normalized_hash(text);
        let now = now_ms();

        let conn = self.conn.lock().expect("facts store poisoned");
        // Exact duplicate (any validity) → NOOP.
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM facts WHERE hash = ?1", params![hash], |r| {
                r.get(0)
            })
            .optional()?;
        if exists.is_some() {
            return Ok(AddOutcome::Duplicate);
        }

        // Compare against CURRENT (non-superseded) facts.
        let mut best: Option<(i64, f32)> = None;
        {
            let mut stmt =
                conn.prepare("SELECT id, embedding FROM facts WHERE valid_to IS NULL")?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
            for row in rows {
                let (id, blob) = row?;
                let existing = decode_embedding(&blob);
                if existing.len() != self.dim {
                    continue;
                }
                let score = cosine(embedding, &existing);
                if best.map(|(_, s)| score > s).unwrap_or(true) {
                    best = Some((id, score));
                }
            }
        }

        let outcome = match best {
            Some((_, score)) if score >= DUP_THRESHOLD => return Ok(AddOutcome::Duplicate),
            Some((id, score)) if score >= SUPERSEDE_THRESHOLD => {
                conn.execute(
                    "UPDATE facts SET valid_to = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                AddOutcome::Superseded { previous_id: id }
            }
            _ => AddOutcome::Added,
        };
        let new_id = insert_row(&conn, meeting_id, text, &hash, now, embedding)?;
        match outcome {
            AddOutcome::Superseded { previous_id } => {
                let old_text = fact_text(&conn, previous_id)?;
                log_history(
                    &conn,
                    new_id,
                    old_text.as_deref(),
                    Some(text),
                    "UPDATE",
                    false,
                    now,
                )?;
            }
            _ => log_history(&conn, new_id, None, Some(text), "ADD", false, now)?,
        }
        Ok(outcome)
    }

    /// ADD op (agent-decided): insert as a new current fact. `None` when the
    /// exact-normalized text already exists (any validity) — the pre-agent
    /// hash dedup should normally have filtered these.
    pub fn insert_fact(
        &self,
        meeting_id: &str,
        text: &str,
        embedding: &[f32],
    ) -> Result<Option<i64>> {
        anyhow::ensure!(embedding.len() == self.dim, "embedding dim mismatch");
        let text = text.trim();
        anyhow::ensure!(!text.is_empty(), "empty fact");
        let hash = normalized_hash(text);
        let now = now_ms();
        let conn = self.conn.lock().expect("facts store poisoned");
        if hash_exists(&conn, &hash)? {
            return Ok(None);
        }
        let id = insert_row(&conn, meeting_id, text, &hash, now, embedding)?;
        log_history(&conn, id, None, Some(text), "ADD", false, now)?;
        Ok(Some(id))
    }

    /// UPDATE op (agent-decided): the agent merged/corrected an existing fact.
    /// Zep-style supersede — close the old row's validity window, insert the
    /// revised text as a new current row. Returns the new row id; `None` when
    /// `old_id` is not a current fact (already superseded by a concurrent op).
    pub fn supersede_fact(
        &self,
        old_id: i64,
        meeting_id: &str,
        new_text: &str,
        new_embedding: &[f32],
    ) -> Result<Option<i64>> {
        anyhow::ensure!(new_embedding.len() == self.dim, "embedding dim mismatch");
        let new_text = new_text.trim();
        anyhow::ensure!(!new_text.is_empty(), "empty fact");
        let now = now_ms();
        let conn = self.conn.lock().expect("facts store poisoned");
        let closed = conn.execute(
            "UPDATE facts SET valid_to = ?1 WHERE id = ?2 AND valid_to IS NULL",
            params![now, old_id],
        )?;
        if closed == 0 {
            return Ok(None);
        }
        let old_text = fact_text(&conn, old_id)?;
        // Revised text may already exist verbatim (e.g. the agent "merged"
        // into a fact we also hold) — the old row is closed either way; only
        // skip the duplicate insert and log against the row holding the text.
        let hash = normalized_hash(new_text);
        let new_id = match id_by_hash(&conn, &hash)? {
            Some(existing) => existing,
            None => insert_row(&conn, meeting_id, new_text, &hash, now, new_embedding)?,
        };
        log_history(
            &conn,
            new_id,
            old_text.as_deref(),
            Some(new_text),
            "UPDATE",
            false,
            now,
        )?;
        Ok(Some(new_id))
    }

    /// DELETE op (agent-decided contradiction): our supersede delta — close
    /// the validity window, keep the row queryable in history. Returns false
    /// when the id is not a current fact.
    pub fn invalidate_fact(&self, id: i64) -> Result<bool> {
        let now = now_ms();
        let conn = self.conn.lock().expect("facts store poisoned");
        let closed = conn.execute(
            "UPDATE facts SET valid_to = ?1 WHERE id = ?2 AND valid_to IS NULL",
            params![now, id],
        )?;
        if closed == 0 {
            return Ok(false);
        }
        let old_text = fact_text(&conn, id)?;
        log_history(&conn, id, old_text.as_deref(), None, "DELETE", true, now)?;
        Ok(true)
    }

    /// Whether the exact-normalized text is already stored (any validity).
    /// The update phase runs this BEFORE the agent call so already-known facts
    /// short-circuit to NONE without costing a drive (mem0 v3's hash dedup).
    pub fn contains_exact(&self, text: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("facts store poisoned");
        hash_exists(&conn, &normalized_hash(text.trim()))
    }

    /// Top-`k` CURRENT facts by similarity, with row ids — the update phase's
    /// retrieval (presented to the agent behind small display indexes).
    pub fn similar_current(&self, embedding: &[f32], k: usize) -> Result<Vec<FactRow>> {
        anyhow::ensure!(embedding.len() == self.dim, "embedding dim mismatch");
        if k == 0 {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().expect("facts store poisoned");
        let mut stmt =
            conn.prepare("SELECT id, text, embedding FROM facts WHERE valid_to IS NULL")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut out: Vec<FactRow> = Vec::new();
        for row in rows {
            let (id, text, blob) = row?;
            let existing = decode_embedding(&blob);
            if existing.len() != self.dim {
                continue;
            }
            out.push(FactRow {
                id,
                text,
                score: cosine(embedding, &existing),
            });
        }
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(k);
        Ok(out)
    }

    /// Most recent audit rows, newest first (debug surface + tests).
    pub fn recent_history(&self, limit: usize) -> Result<Vec<HistoryRow>> {
        let conn = self.conn.lock().expect("facts store poisoned");
        let mut stmt = conn.prepare(
            "SELECT memory_id, old_memory, new_memory, event, is_deleted, at_ms
             FROM facts_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(HistoryRow {
                memory_id: r.get(0)?,
                old_memory: r.get(1)?,
                new_memory: r.get(2)?,
                event: r.get(3)?,
                is_deleted: r.get::<_, i64>(4)? != 0,
                at_ms: r.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Top-`k` CURRENT facts by cosine similarity, optionally excluding one
    /// meeting (the active meeting's ledger is already pinned in context —
    /// cross-meeting recall should surface OTHER meetings' facts).
    pub fn query(
        &self,
        query_embedding: &[f32],
        k: usize,
        exclude_meeting: Option<&str>,
    ) -> Result<Vec<FactHit>> {
        anyhow::ensure!(query_embedding.len() == self.dim, "embedding dim mismatch");
        if k == 0 {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().expect("facts store poisoned");
        let mut stmt = conn.prepare(
            "SELECT meeting_id, text, created_at, embedding FROM facts WHERE valid_to IS NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        let mut hits: Vec<FactHit> = Vec::new();
        for row in rows {
            let (meeting_id, text, created_at_ms, blob) = row?;
            if exclude_meeting == Some(meeting_id.as_str()) {
                continue;
            }
            let embedding = decode_embedding(&blob);
            if embedding.len() != self.dim {
                continue;
            }
            hits.push(FactHit {
                score: cosine(query_embedding, &embedding),
                text,
                meeting_id,
                created_at_ms,
            });
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k);
        Ok(hits)
    }

    /// Number of CURRENT (non-superseded) facts.
    pub fn current_len(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("facts store poisoned");
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM facts WHERE valid_to IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

fn id_by_hash(conn: &Connection, hash: &str) -> Result<Option<i64>> {
    conn.query_row("SELECT id FROM facts WHERE hash = ?1", params![hash], |r| {
        r.get(0)
    })
    .optional()
    .map_err(Into::into)
}

fn hash_exists(conn: &Connection, hash: &str) -> Result<bool> {
    Ok(id_by_hash(conn, hash)?.is_some())
}

fn fact_text(conn: &Connection, id: i64) -> Result<Option<String>> {
    conn.query_row("SELECT text FROM facts WHERE id = ?1", params![id], |r| {
        r.get(0)
    })
    .optional()
    .map_err(Into::into)
}

fn insert_row(
    conn: &Connection,
    meeting_id: &str,
    text: &str,
    hash: &str,
    now: i64,
    embedding: &[f32],
) -> Result<i64> {
    conn.execute(
        "INSERT INTO facts (meeting_id, text, hash, created_at, valid_to, embedding)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
        params![meeting_id, text, hash, now, encode_embedding(embedding)],
    )?;
    Ok(conn.last_insert_rowid())
}

fn log_history(
    conn: &Connection,
    memory_id: i64,
    old_memory: Option<&str>,
    new_memory: Option<&str>,
    event: &str,
    is_deleted: bool,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO facts_history (memory_id, old_memory, new_memory, event, is_deleted, at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            memory_id,
            old_memory,
            new_memory,
            event,
            is_deleted as i64,
            now
        ],
    )?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Stable content hash over case/whitespace-normalized text (exact dedup).
fn normalized_hash(text: &str) -> String {
    let normalized = text
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // FNV-1a 64 — tiny, dependency-free, adequate for content dedup keys.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in normalized.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn encode_embedding(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn decode_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|v| v * v).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|v| v * v).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mem() -> FactsStore {
        FactsStore::open(&PathBuf::from(":memory:"), 3).expect("open")
    }

    #[test]
    fn add_query_and_exclude_meeting() {
        let store = mem();
        store
            .add_fact("m1", "shard the db by tenant id", &[1.0, 0.0, 0.0])
            .unwrap();
        store
            .add_fact("m2", "checkout latency sla is 200ms", &[0.0, 1.0, 0.0])
            .unwrap();

        let hits = store.query(&[1.0, 0.05, 0.0], 5, None).unwrap();
        assert_eq!(hits[0].text, "shard the db by tenant id");

        // Excluding the fact's own meeting removes it from recall.
        let hits = store.query(&[1.0, 0.05, 0.0], 5, Some("m1")).unwrap();
        assert!(hits.iter().all(|h| h.meeting_id != "m1"));
    }

    #[test]
    fn exact_and_near_duplicates_are_noops() {
        let store = mem();
        assert_eq!(
            store
                .add_fact("m1", "Use gRPC internally", &[1.0, 0.0, 0.0])
                .unwrap(),
            AddOutcome::Added
        );
        // Same text, different case/spacing → exact-hash duplicate.
        assert_eq!(
            store
                .add_fact("m2", "use  grpc INTERNALLY", &[0.9, 0.1, 0.0])
                .unwrap(),
            AddOutcome::Duplicate
        );
        // Different text but near-identical embedding → near-duplicate NOOP.
        assert_eq!(
            store
                .add_fact("m2", "internal calls use grpc", &[0.999, 0.01, 0.0])
                .unwrap(),
            AddOutcome::Duplicate
        );
        assert_eq!(store.current_len().unwrap(), 1);
    }

    #[test]
    fn same_topic_revision_supersedes_not_deletes() {
        let store = mem();
        store
            .add_fact("m1", "we chose postgres for new services", &[1.0, 0.0, 0.0])
            .unwrap();
        // Same topic (cosine ~0.89 with [0.9, 0.44, 0]) → supersede.
        let outcome = store
            .add_fact("m3", "we switched new services to mongo", &[0.9, 0.44, 0.0])
            .unwrap();
        assert!(matches!(outcome, AddOutcome::Superseded { .. }));

        // Only the NEW fact is current; recall returns it, not the old one.
        assert_eq!(store.current_len().unwrap(), 1);
        let hits = store.query(&[1.0, 0.1, 0.0], 5, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.contains("mongo"));
    }

    #[test]
    fn unrelated_facts_all_stay_current() {
        let store = mem();
        store.add_fact("m1", "a", &[1.0, 0.0, 0.0]).unwrap();
        store.add_fact("m1", "b", &[0.0, 1.0, 0.0]).unwrap();
        store.add_fact("m2", "c", &[0.0, 0.0, 1.0]).unwrap();
        assert_eq!(store.current_len().unwrap(), 3);
    }

    #[test]
    fn agent_ops_supersede_invalidate_and_audit() {
        let store = mem();
        let id = store
            .insert_fact("m1", "sla is 200ms", &[1.0, 0.0, 0.0])
            .unwrap()
            .expect("added");

        // UPDATE: revised text replaces the old row; old stays in history.
        let new_id = store
            .supersede_fact(id, "m2", "sla moved to 300ms", &[0.9, 0.1, 0.0])
            .unwrap()
            .expect("superseded");
        assert_ne!(new_id, id);
        assert_eq!(store.current_len().unwrap(), 1);
        let current = store.similar_current(&[1.0, 0.0, 0.0], 5).unwrap();
        assert_eq!(current.len(), 1);
        assert!(current[0].text.contains("300ms"));

        // A second UPDATE against the already-closed id is a no-op.
        assert!(store
            .supersede_fact(id, "m2", "sla 400ms", &[0.8, 0.2, 0.0])
            .unwrap()
            .is_none());

        // DELETE: closes the window, keeps history.
        assert!(store.invalidate_fact(new_id).unwrap());
        assert!(!store.invalidate_fact(new_id).unwrap(), "already closed");
        assert_eq!(store.current_len().unwrap(), 0);

        // Audit trail: ADD, UPDATE, DELETE — newest first; NONE never logged.
        let history = store.recent_history(10).unwrap();
        let events: Vec<&str> = history.iter().map(|h| h.event.as_str()).collect();
        assert_eq!(events, vec!["DELETE", "UPDATE", "ADD"]);
        assert!(history[0].is_deleted);
        assert_eq!(history[1].old_memory.as_deref(), Some("sla is 200ms"));
        assert_eq!(history[1].new_memory.as_deref(), Some("sla moved to 300ms"));
    }

    #[test]
    fn insert_fact_exact_dup_is_none_and_contains_exact_sees_it() {
        let store = mem();
        store
            .insert_fact("m1", "Use gRPC internally", &[1.0, 0.0, 0.0])
            .unwrap()
            .expect("added");
        assert!(store
            .insert_fact("m2", "use  grpc INTERNALLY", &[0.9, 0.1, 0.0])
            .unwrap()
            .is_none());
        assert!(store.contains_exact("USE GRPC internally").unwrap());
        assert!(!store.contains_exact("use rest externally").unwrap());
    }
}
