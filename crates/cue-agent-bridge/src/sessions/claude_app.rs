//! Reader for the Claude **desktop app**'s session index (`ClaudeAppIndex`).
//!
//! The Claude app (Code mode and agent/cowork mode) shares the Claude Code
//! *engine and transcript store* with the `claude` CLI, but it maintains its
//! own per-session **index** files under
//! `Library/Application Support/Claude/{claude-code-sessions,local-agent-mode-sessions}/<account>/<workspace>/local_*.json`.
//!
//! Each `local_*.json` is metadata only — a rich pre-computed `title`, `model`,
//! `cwd`, `lastActivityAt`, a `completedTurns` *count* (not the turns), and
//! crucially a `cliSessionId`. The actual conversation lives in the shared
//! Claude Code JSONL store at
//! `~/.claude/projects/<encoded-cwd>/<cliSessionId>.jsonl`.
//!
//! So this is a **two-hop reader**:
//! 1. `list` parses the index files for titles + the `cliSessionId` (used as the
//!    [`SessionRef::id`] so it both resolves the body below *and* lets the CLI
//!    row de-duplicate against app-claimed sessions).
//! 2. `read` follows `cliSessionId` into the shared JSONL and reuses
//!    [`super::jsonl::read_transcript_file`] for identical line decoding.
//!
//! Security invariants match every other reader: read-only `std::fs`, bounded,
//! fail-soft (a malformed/oversized index file is skipped, never fatal), and no
//! secrets — only title/metadata/transcript text.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{clean_title, fallback_label, mtime_epoch_string, SessionReader};
use crate::{SessionRef, SessionStore, Transcript};

/// Cap on index files scanned, so a giant store can't stall a `list`.
const MAX_INDEX_FILES: usize = 2_000;
/// An index file larger than this is not a normal metadata blob; skip it.
const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;
/// Max chars kept for a derived session title.
const TITLE_MAX_CHARS: usize = 90;

/// The fields we read from a Claude-app `local_*.json` index file. Everything
/// else in the file (permissions, MCP config, computer-use flags) is ignored.
#[derive(Debug, Deserialize)]
struct AppIndex {
    /// The shared Claude Code transcript id — the second hop's filename stem.
    #[serde(rename = "cliSessionId")]
    cli_session_id: Option<String>,
    /// Pre-computed human title (e.g. "Bluey repository setup"). Preferred over
    /// deriving from the first turn — it is already clean.
    title: Option<String>,
    /// Working directory the session ran in; encodes the JSONL project dir.
    cwd: Option<String>,
    /// Whether the user archived the session (kept out of the default list).
    #[serde(rename = "isArchived", default)]
    is_archived: bool,
    /// Count of completed turns (NOT the turns themselves). Used to drop empty
    /// sessions from the listing.
    #[serde(rename = "completedTurns", default)]
    completed_turns: u64,
}

/// Reader for [`SessionFormat::ClaudeAppIndex`](crate::SessionFormat::ClaudeAppIndex).
pub struct ClaudeAppReader;

impl SessionReader for ClaudeAppReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let mut refs: Vec<(SessionRef, String)> = enumerate_index_files(&store.path)
            .into_iter()
            .filter_map(|file| session_ref_for(&file))
            .collect();
        // Most-recent first by the index file's mtime epoch string.
        refs.sort_by(|a, b| {
            let an = a.0.updated_at.parse::<u64>().unwrap_or(0);
            let bn = b.0.updated_at.parse::<u64>().unwrap_or(0);
            bn.cmp(&an)
        });
        refs.truncate(limit);
        Ok(refs.into_iter().map(|(r, _)| r).collect())
    }

    fn read(&self, store: &SessionStore, id: &str, max_turns: usize) -> anyhow::Result<Transcript> {
        // `id` is the cliSessionId. Find the index file that owns it so we can
        // recover its `cwd` (needed to locate the JSONL project dir).
        let Some(cwd) = cwd_for_cli_session(&store.path, id) else {
            // No index entry → nothing to read. Fail-soft to empty.
            return Ok(Transcript::default());
        };
        let jsonl = claude_projects_jsonl(&cwd, id);
        if !jsonl.is_file() {
            return Ok(Transcript::default());
        }
        super::jsonl::read_transcript_file(&jsonl, max_turns)
    }
}

/// Enumerate `local_*.json` index files under `<store>/<account>/<workspace>/`.
/// Read-only, fail-soft, bounded by [`MAX_INDEX_FILES`].
fn enumerate_index_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_index(root, 3, &mut out);
    out
}

/// Recurse to a bounded depth collecting `local_*.json` files. The store layout
/// is `<root>/<account>/<workspace>/local_*.json` (depth 2), but we allow a
/// little slack in case the account/workspace nesting varies by app version.
fn collect_index(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= MAX_INDEX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_INDEX_FILES {
            return;
        }
        let p = entry.path();
        if p.is_file() {
            if is_index_file(&p) {
                out.push(p);
            }
        } else if p.is_dir() && depth > 0 {
            collect_index(&p, depth - 1, out);
        }
    }
}

/// A session index file is named `local_*.json`. The app also drops sidecar
/// JSON (`cowork_settings.json`, `scheduled-tasks.json`, `.claude.json`); those
/// are not `local_`-prefixed and so are excluded.
fn is_index_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "json")
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("local_"))
}

/// Parse one index file into a [`SessionRef`]. Returns `None` (skipped) for an
/// oversized/unreadable/unparseable file, an archived session, an empty session
/// (zero turns), or one without a `cliSessionId` (no body to resolve later).
/// The returned tuple's second element is the resolved `cwd`, kept for callers
/// that want it (currently unused by `list`, but cheap and explicit).
fn session_ref_for(file: &Path) -> Option<(SessionRef, String)> {
    let meta = std::fs::metadata(file).ok()?;
    if meta.len() > MAX_INDEX_BYTES {
        return None;
    }
    let bytes = std::fs::read(file).ok()?;
    let index: AppIndex = serde_json::from_slice(&bytes).ok()?;

    if index.is_archived || index.completed_turns == 0 {
        return None;
    }
    let cli_session_id = index.cli_session_id?;
    let cwd = index.cwd.unwrap_or_default();

    // App title is already clean; still run it through clean_title to strip any
    // stray wrapper tags / reject boilerplate, then fall back to the project.
    let title =
        clean_title(index.title, TITLE_MAX_CHARS).or_else(|| fallback_label(Some(&cwd), ""));

    let session_ref = SessionRef {
        // Use the CLI session id as the ref id: it resolves the body in `read`
        // AND is the key the CLI row de-dups against (so the same conversation
        // doesn't appear under both "Claude Code (App)" and "Claude Code (CLI)").
        id: cli_session_id,
        title,
        updated_at: mtime_epoch_string(file),
        project: if cwd.is_empty() {
            None
        } else {
            Some(cwd.clone())
        },
    };
    Some((session_ref, cwd))
}

/// Scan index files for the one whose `cliSessionId == id`, returning its `cwd`.
fn cwd_for_cli_session(root: &Path, id: &str) -> Option<String> {
    for file in enumerate_index_files(root) {
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let Ok(index) = serde_json::from_slice::<AppIndex>(&bytes) else {
            continue;
        };
        if index.cli_session_id.as_deref() == Some(id) {
            return Some(index.cwd.unwrap_or_default());
        }
    }
    None
}

/// Build the shared Claude Code transcript path for a `(cwd, cliSessionId)`:
/// `~/.claude/projects/<encoded-cwd>/<cliSessionId>.jsonl`.
fn claude_projects_jsonl(cwd: &str, cli_session_id: &str) -> PathBuf {
    let home = home_dir();
    home.join(".claude")
        .join("projects")
        .join(encode_project_dir(cwd))
        .join(format!("{cli_session_id}.jsonl"))
}

/// Encode an absolute `cwd` into Claude Code's project-dir name: **every
/// non-alphanumeric character** becomes `-` (so `/Users/ms/Developer/Bluey` →
/// `-Users-ms-Developer-Bluey`, a `/.hidden` segment yields `--hidden`, and a
/// path with spaces/apostrophes like `/…/Divi's Agenda` → `-…-Divi-s-Agenda`).
/// This mirrors Claude Code's own on-disk encoding (the Agent SDK source uses
/// the regex `[^a-zA-Z0-9] → -`) and is the inverse of `jsonl::decode_project_dir`.
///
/// Previously this mapped only `/` and `.`, so any project whose path contained
/// a space, apostrophe, underscore, parenthesis, etc. encoded to a directory
/// that does not exist on disk → the second hop read an empty transcript. This
/// is a strict superset of the old mapping (`/` and `.` are themselves
/// non-alphanumeric), so paths of only letters/digits/`/`/`.` are unaffected.
fn encode_project_dir(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `$HOME`, or `/` if unset (the resulting path simply won't exist → empty read).
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn encode_project_dir_matches_claude_layout() {
        assert_eq!(
            encode_project_dir("/Users/ms/Developer/Bluey"),
            "-Users-ms-Developer-Bluey"
        );
        // A hidden segment: '/' then '.' → '--'.
        assert_eq!(
            encode_project_dir("/Users/ms/.config/x"),
            "-Users-ms--config-x"
        );
        // Spaces and apostrophes are non-alphanumeric → '-' (real case:
        // "/Users/ms/Developer/Divi's Agenda" was reading an empty transcript
        // before, because the old mapping left " " and "'" untouched).
        assert_eq!(
            encode_project_dir("/Users/ms/Developer/Divi's Agenda"),
            "-Users-ms-Developer-Divi-s-Agenda"
        );
        // Underscores and parens too.
        assert_eq!(encode_project_dir("/a/my_proj (v2)"), "-a-my-proj--v2-");
    }

    #[test]
    fn list_uses_app_title_skips_empty_and_archived() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("acct").join("ws");
        // A real session with a rich title.
        write(
            &ws.join("local_a.json"),
            r#"{"cliSessionId":"abc-123","title":"Bluey repository setup","cwd":"/Users/ms/Developer/Bluey","completedTurns":139,"isArchived":false}"#,
        );
        // Empty session (0 turns) → skipped.
        write(
            &ws.join("local_b.json"),
            r#"{"cliSessionId":"def-456","title":"Throwaway","cwd":"/x","completedTurns":0}"#,
        );
        // Archived → skipped.
        write(
            &ws.join("local_c.json"),
            r#"{"cliSessionId":"ghi-789","title":"Old","cwd":"/x","completedTurns":5,"isArchived":true}"#,
        );
        // Sidecar (not local_) → not even considered.
        write(&ws.join("cowork_settings.json"), r#"{"foo":"bar"}"#);

        let store = SessionStore {
            path: dir.path().to_path_buf(),
            format: crate::SessionFormat::ClaudeAppIndex,
        };
        let refs = ClaudeAppReader.list(&store, 10).unwrap();
        assert_eq!(refs.len(), 1, "only the non-empty, non-archived session");
        assert_eq!(refs[0].id, "abc-123", "id is the cliSessionId");
        assert_eq!(refs[0].title.as_deref(), Some("Bluey repository setup"));
        assert_eq!(
            refs[0].project.as_deref(),
            Some("/Users/ms/Developer/Bluey")
        );
    }

    #[test]
    fn read_returns_empty_when_no_index_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore {
            path: dir.path().to_path_buf(),
            format: crate::SessionFormat::ClaudeAppIndex,
        };
        // Unknown id → fail-soft empty, never an error/panic.
        let t = ClaudeAppReader.read(&store, "nope", 100).unwrap();
        assert!(t.turns.is_empty());
    }
}
