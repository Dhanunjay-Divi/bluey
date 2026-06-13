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

pub mod claude_app;
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
        SessionFormat::ClaudeAppIndex => Box::new(claude_app::ClaudeAppReader),
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

/// Whether a candidate first-message is **boilerplate**, not a real topic —
/// shared by every reader so all agents reject the same junk titles. Catches
/// session-continuation banners, system/agent prompts, and Bluey's own
/// propose/apply/summary prompts that would otherwise become the "title".
///
/// Cross-agent by design: Claude, Codex, Cursor, VS Code all surface some of
/// these, so the rule lives here, not per-reader.
pub(crate) fn is_boilerplate_title(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    let lower = t.to_lowercase();
    const MARKERS: &[&str] = &[
        "this session is being continued",
        "continued from a previous conversation",
        "you are proposing a fix",
        "propose-only",
        "===diagnosis===",
        "one sentence: what is this session about", // Bluey's own summary prompt
        "summarize what this",
        "this is the gemini cli",
        "<session_context>",
        "<environment_context>", // Codex injects this as the first turn
        "<cwd>",
        "<user_instructions>",
        "you are a ", // system role prompts ("You are a Rust architect…")
        "system:",
        "caveat: the messages below", // Claude Code system caveat banner
    ];
    MARKERS
        .iter()
        .any(|m| lower.starts_with(m) || lower.contains(m))
}

/// A meaningful fallback label when no real title is available — uses the
/// project's last path segment (e.g. `…/Developer/Bluey` → "Bluey") so the row
/// is identifiable instead of blank or a raw id/filename. Returns `None` only
/// when there is no project to derive from (the caller then shows a short id).
/// `_updated_at` is accepted for a future date suffix (no date lib in-tree yet).
pub(crate) fn fallback_label(project: Option<&str>, _updated_at: &str) -> Option<String> {
    let project = project?;
    let leaf = project
        .trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())?;
    if leaf.is_empty() {
        None
    } else {
        Some(format!("{leaf} session"))
    }
}

/// Pick a human title for a session: the candidate first-message if it is a
/// real topic; else `None` so the caller falls back to project + date. Applied
/// uniformly across readers.
pub(crate) fn clean_title(candidate: Option<String>, max_chars: usize) -> Option<String> {
    let text = strip_wrapper_tags(&candidate?);
    if is_boilerplate_title(&text) {
        return None;
    }
    let s = snippet(&text, max_chars);
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Strip XML-ish wrapper tags some agents wrap user content in (e.g.
/// Antigravity's `<USER_REQUEST>…</USER_REQUEST>`, Codex's `<user_instructions>`)
/// so the title shows the actual message, not the tag. Generic — applies to any
/// agent; leaves normal text (and inline `<` in code) intact by only removing
/// whole `<TAG>`/`</TAG>` tokens.
pub(crate) fn strip_wrapper_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            // Consume a tag only if it looks like <word ...> or </word> — a
            // letter/slash right after '<'. Otherwise keep the '<' (e.g. `a < b`).
            if matches!(chars.peek(), Some(n) if n.is_ascii_alphabetic() || *n == '/') {
                for t in chars.by_ref() {
                    if t == '>' {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
