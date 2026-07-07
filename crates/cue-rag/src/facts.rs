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
//! v1 consolidation is this similarity heuristic (deliberate: no extra LLM call
//! per fact); the extraction step is already LLM-verified. An LLM-judged
//! ADD/UPDATE/DELETE pass is the documented follow-up if the heuristic proves
//! too coarse in real use.

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
            CREATE INDEX IF NOT EXISTS idx_facts_valid ON facts(valid_to);",
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
        conn.execute(
            "INSERT INTO facts (meeting_id, text, hash, created_at, valid_to, embedding)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![meeting_id, text, hash, now, encode_embedding(embedding)],
        )?;
        Ok(outcome)
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
}
