//! Spawn-time **agent-session ledger** — a fail-soft JSONL record of the
//! sessions Bluey ITSELF minted through an agent CLI, so a later resume can
//! recover the session's effective cwd WITHOUT re-scraping the vendor's on-disk
//! store (which drifts across agent releases).
//!
//! Why this exists: a cwd-scoped native resume (Claude `--resume`, Cursor
//! `--resume=<id>`) resolves against the session's ORIGINAL working directory,
//! and recovering that directory from the vendor store is exactly the read the
//! store-format drift breaks. The daemon appends one record here at spawn time
//! (the id + effective cwd are both in scope then); [`crate::continuation::tier`]
//! reads it back BEFORE touching the store, so the cwd survives drift.
//!
//! Design (deliberately minimal — this is an OPTIMIZATION layer, never a
//! replacement for the store re-scrape):
//! - **Format**: JSONL, one [`SessionLedgerRecord`] per line, append-only.
//! - **No fsync**: a lost tail line on power loss degrades to the store
//!   re-scrape (the miss path), never loses user data — so per-turn fsync would
//!   add latency to the live answer path for no correctness gain.
//! - **No locking**: each [`append`] is a single `O_APPEND` `write_all` of a
//!   sub-200-byte line; whole-line interleaving is atomic enough in practice and
//!   the corrupt-line-skip reader makes a torn tail non-fatal.
//! - **No rotation / compaction**: growth is ~150 bytes per NEW session id, so
//!   multi-year use is single-digit MB; keep-latest dedup happens on READ. If a
//!   size check ever shows multi-MB files, revisit.
//!
//! The bridge stays `cue-core`-free: this module uses only `std` + the serde /
//! anyhow deps the crate already carries. The daemon composes the storage path
//! from its own `AppPaths` and injects it — the dependency arrow stays
//! daemon → bridge.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::AgentKind;

/// File name of the spawn-time agent-session ledger, created under the daemon's
/// data dir. Deliberately named `agent-session-*` to disambiguate from the
/// unrelated meeting DECISIONS ledger.
pub const AGENT_SESSION_LEDGER_FILE: &str = "agent-session-ledger.jsonl";

/// One spawn-time record: which agent minted which session, in which cwd, when.
///
/// `agent` serializes with the same stable snake_case string as
/// `settings.attached_agent` (via [`AgentKind`]'s serde), so a lookup keyed on
/// the settings value matches without translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionLedgerRecord {
    /// The agent that minted the session (serde snake_case, same string as
    /// `settings.attached_agent`).
    pub agent: AgentKind,
    /// The native session id the agent reported (`AnswerChunk::Started`).
    pub session_id: String,
    /// The EFFECTIVE drive cwd — `question.cwd` after the continuation tier ran,
    /// falling back to the inherited cwd. `None` only if neither was resolvable.
    pub cwd: Option<String>,
    /// Spawn time, unix millis.
    pub spawned_at: u64,
}

impl SessionLedgerRecord {
    /// Build a record, stamping `spawned_at` from the system clock. A pre-epoch
    /// clock yields `0` rather than panicking (the timestamp is advisory only).
    #[must_use]
    pub fn new(agent: AgentKind, session_id: impl Into<String>, cwd: Option<String>) -> Self {
        let spawned_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            agent,
            session_id: session_id.into(),
            cwd,
            spawned_at,
        }
    }
}

/// Append `record` as one JSONL line. Creates the file if missing (`O_APPEND`),
/// writes the serialized line + `'\n'` in a single `write_all`. No fsync (the
/// ledger is a resume optimization; a lost tail line degrades to the store
/// re-scrape). Returns `Err` only on a serialize or IO failure — the caller
/// warns-and-continues so a ledger fault never fails the answer.
pub fn append(path: &Path, record: &SessionLedgerRecord) -> anyhow::Result<()> {
    let mut line = serde_json::to_string(record)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

/// Read every record. A missing file → empty `Vec` (not an error). Malformed
/// lines are skipped (fail-soft, the same invariant every `SessionReader`
/// honors). Records are deduped on `(agent, session_id)` keeping the LAST
/// occurrence (append order = keep-latest), so a re-minted id or a corrected cwd
/// supersedes the earlier line.
#[must_use]
pub fn read_all(path: &Path) -> Vec<SessionLedgerRecord> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new(); // missing / unreadable → empty, fail-soft
    };
    // Keep the LAST record per (agent, session_id), preserving first-seen order
    // of the survivors. `AgentKind` is only `PartialEq` (no `Hash`), so this uses
    // a linear scan — fine at MVP ledger sizes (a once-per-continuation read).
    let mut out: Vec<SessionLedgerRecord> = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(record) = serde_json::from_str::<SessionLedgerRecord>(trimmed) else {
            continue; // skip a torn / drifted line, keep the rest
        };
        match out
            .iter_mut()
            .find(|r| r.agent == record.agent && r.session_id == record.session_id)
        {
            Some(existing) => *existing = record, // later line supersedes (keep-latest)
            None => out.push(record),
        }
    }
    out
}

/// Look up the (deduped, keep-latest) record for one `(agent, session_id)`.
/// `None` when there is no matching record (or the file is missing). `O(n)` —
/// fine at MVP ledger sizes.
#[must_use]
pub fn lookup(path: &Path, agent: &AgentKind, session_id: &str) -> Option<SessionLedgerRecord> {
    read_all(path)
        .into_iter()
        .find(|r| &r.agent == agent && r.session_id == session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_ledger(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("bluey-ledger-test-{name}-{unique}.jsonl"));
        p
    }

    #[test]
    fn test_ledger_append_then_read_round_trips_record() {
        let path = tmp_ledger("round-trip");
        let rec = SessionLedgerRecord::new(
            AgentKind::ClaudeCode,
            "sess-1",
            Some("/tmp/proj".to_string()),
        );
        append(&path, &rec).expect("append");
        let all = read_all(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], rec, "record round-trips field-for-field");
    }

    #[test]
    fn test_ledger_read_dedups_same_agent_session_keeping_latest() {
        let path = tmp_ledger("dedup");
        let first = SessionLedgerRecord {
            agent: AgentKind::Codex,
            session_id: "id-x".to_string(),
            cwd: Some("/old".to_string()),
            spawned_at: 1,
        };
        let second = SessionLedgerRecord {
            agent: AgentKind::Codex,
            session_id: "id-x".to_string(),
            cwd: Some("/new".to_string()),
            spawned_at: 2,
        };
        append(&path, &first).expect("append 1");
        append(&path, &second).expect("append 2");
        let all = read_all(&path);
        let hit = lookup(&path, &AgentKind::Codex, "id-x");
        let _ = std::fs::remove_file(&path);
        assert_eq!(all.len(), 1, "same (agent, id) collapses to one record");
        assert_eq!(all[0].cwd, Some("/new".to_string()), "keeps the LATEST");
        assert_eq!(hit.and_then(|r| r.cwd), Some("/new".to_string()));
    }

    #[test]
    fn test_ledger_read_skips_corrupt_lines_and_returns_valid_ones() {
        let path = tmp_ledger("corrupt");
        let a = SessionLedgerRecord::new(AgentKind::Cursor, "good-1", None);
        let b = SessionLedgerRecord::new(AgentKind::Cursor, "good-2", None);
        append(&path, &a).expect("append a");
        // Hand-write a garbage line between the two valid ones.
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open");
            f.write_all(b"{ this is not json\n").expect("write garbage");
        }
        append(&path, &b).expect("append b");
        let all = read_all(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(all.len(), 2, "both valid records survive the corrupt line");
        assert_eq!(all[0].session_id, "good-1");
        assert_eq!(all[1].session_id, "good-2");
    }

    #[test]
    fn test_ledger_read_missing_file_returns_empty() {
        let path = tmp_ledger("missing");
        // Never created.
        assert!(
            read_all(&path).is_empty(),
            "missing file → empty Vec, no error"
        );
        assert!(lookup(&path, &AgentKind::ClaudeCode, "any").is_none());
    }

    #[test]
    fn test_ledger_lookup_distinguishes_same_session_id_across_agents() {
        let path = tmp_ledger("cross-agent");
        let claude = SessionLedgerRecord::new(
            AgentKind::ClaudeCode,
            "shared-id",
            Some("/claude".to_string()),
        );
        let codex =
            SessionLedgerRecord::new(AgentKind::Codex, "shared-id", Some("/codex".to_string()));
        append(&path, &claude).expect("append claude");
        append(&path, &codex).expect("append codex");
        let claude_hit = lookup(&path, &AgentKind::ClaudeCode, "shared-id");
        let codex_hit = lookup(&path, &AgentKind::Codex, "shared-id");
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            claude_hit.and_then(|r| r.cwd),
            Some("/claude".to_string()),
            "same id under ClaudeCode resolves to its own record"
        );
        assert_eq!(
            codex_hit.and_then(|r| r.cwd),
            Some("/codex".to_string()),
            "same id under Codex resolves to its own record"
        );
    }
}
