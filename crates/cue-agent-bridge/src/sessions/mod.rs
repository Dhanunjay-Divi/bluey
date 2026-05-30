//! Per-agent session-store readers (Slice 3).
//!
//! Each installed agent persists its conversation history in its own on-disk
//! format (see `PLAN-AGENT-BRIDGE.md` §3). This module defines a single
//! [`SessionReader`] abstraction and one decoder per [`SessionFormat`]:
//!
//! - [`jsonl`] — Claude Code / Codex newline-delimited JSON.
//! - [`vscdb`] — Cursor / VS Code-family `state.vscdb` SQLite store.
//! - [`json_files`] — VS Code / Copilot `chatSessions/*.json` files.
//! - [`protobuf`] — Antigravity `*.pb` (safe stub; wire format unknown).
//!
//! Security invariants enforced by every decoder (design §6):
//!
//! - **Read-only.** SQLite opens with `SQLITE_OPEN_READ_ONLY` + `immutable=1`;
//!   file formats read via `std::fs` only. Nothing here ever writes, locks, or
//!   spawns a subprocess.
//! - **Bounded & lazy.** `limit` and `max_turns` are honored; the multi-GB
//!   `vscdb` is queried with SQL `LIMIT`, never slurped into memory.
//! - **Fail-soft.** A malformed line / row / file is skipped; the rest still
//!   returns. No decoder panics on bad input, and none uses `unwrap`/`expect`.
//! - **No secrets.** Only conversation text, titles, and timestamps are read.

use crate::{SessionFormat, SessionRef, SessionStore, Transcript};

pub mod json_files;
pub mod jsonl;
pub mod protobuf;
pub mod vscdb;

/// Decodes one agent's session store, read-only and bounded.
///
/// Implementations must be fail-soft (skip malformed input) and must never
/// write to or lock the underlying store.
pub trait SessionReader {
    /// The `limit` most-recent sessions, cheaply — avoid full-body decode where
    /// the format permits (file mtime / metadata only).
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>>;

    /// Decode a single session identified by `id` into a normalized
    /// [`Transcript`], bounded to at most `max_turns` turns.
    fn read(&self, store: &SessionStore, id: &str, max_turns: usize) -> anyhow::Result<Transcript>;
}

/// Return the decoder for a given storage [`SessionFormat`].
pub fn reader_for(format: SessionFormat) -> Box<dyn SessionReader> {
    match format {
        SessionFormat::Jsonl => Box::new(jsonl::JsonlReader),
        SessionFormat::SqliteVscdb => Box::new(vscdb::VscdbReader),
        SessionFormat::JsonFiles => Box::new(json_files::JsonFilesReader),
        SessionFormat::Protobuf => Box::new(protobuf::ProtobufReader),
    }
}

/// Best-effort last-modified marker for a path, as an epoch-seconds string.
///
/// `SessionRef::updated_at` is documented as "RFC3339 or epoch string, per
/// source"; the file-backed decoders use epoch seconds to stay dependency-free.
/// Returns `"0"` when the mtime is unavailable rather than failing the listing.
pub(crate) fn mtime_epoch_string(path: &std::path::Path) -> String {
    use std::time::UNIX_EPOCH;
    let secs = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

/// Truncate a single-line snippet for use as a session title.
///
/// Collapses internal whitespace and caps length so a title never carries a
/// huge message body. Char-boundary safe.
pub(crate) fn snippet(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(max_chars).collect();
    out.push('…');
    out
}
