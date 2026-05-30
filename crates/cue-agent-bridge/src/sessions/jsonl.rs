//! Newline-delimited JSON decoder for Claude Code and Codex sessions.
//!
//! Storage layout (design §3):
//! - Claude Code: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`
//! - Codex:       `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
//!
//! Each line is a standalone JSON object describing a turn or event. Shapes
//! vary across versions, so parsing is intentionally tolerant: any line that
//! does not yield a usable role + text is skipped, and the rest still decode.
//!
//! For a [`SessionStore`] whose `path` is a *directory*, every `*.jsonl` file
//! under it is one session (id = file stem). If `path` points directly at a
//! `*.jsonl` file, that single file is the only session.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{mtime_epoch_string, snippet, SessionReader};
use crate::{Role, SessionRef, SessionStore, Transcript, Turn};

/// Decoder for Claude Code / Codex JSONL session files.
pub struct JsonlReader;

const TITLE_SNIPPET_CHARS: usize = 80;

impl SessionReader for JsonlReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let mut refs: Vec<SessionRef> = enumerate_files(&store.path)
            .into_iter()
            .filter_map(|file| session_ref_for(&file))
            .collect();
        // Most-recent first by mtime epoch string (zero-padded compare is fine
        // for equal-width epochs; fall back to numeric for safety).
        refs.sort_by(|a, b| {
            let an = a.updated_at.parse::<u64>().unwrap_or(0);
            let bn = b.updated_at.parse::<u64>().unwrap_or(0);
            bn.cmp(&an)
        });
        refs.truncate(limit);
        Ok(refs)
    }

    fn read(&self, store: &SessionStore, id: &str, max_turns: usize) -> anyhow::Result<Transcript> {
        let file = resolve_file(&store.path, id);
        let mut turns = Vec::new();
        if max_turns == 0 {
            return Ok(Transcript { turns });
        }
        let contents = std::fs::read_to_string(&file)
            .map_err(|_| crate::BridgeError::Unreadable(file.clone()))?;
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Skip malformed lines; never abort the whole transcript.
            let value: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(turn) = turn_from_value(&value) {
                turns.push(turn);
                if turns.len() >= max_turns {
                    break;
                }
            }
        }
        Ok(Transcript { turns })
    }
}

/// Enumerate candidate `*.jsonl` files for a store path (file or directory).
fn enumerate_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e == "jsonl") {
                files.push(p);
            }
        }
    }
    files
}

/// Resolve a session `id` (file stem) back to its `*.jsonl` path.
fn resolve_file(path: &Path, id: &str) -> PathBuf {
    if path.is_file() {
        return path.to_path_buf();
    }
    path.join(format!("{id}.jsonl"))
}

/// Build a [`SessionRef`] from a file: id = stem, updated_at = mtime epoch,
/// title = first user-message snippet if cheaply available.
fn session_ref_for(file: &Path) -> Option<SessionRef> {
    let id = file.file_stem()?.to_string_lossy().into_owned();
    let updated_at = mtime_epoch_string(file);
    let title = first_user_snippet(file);
    Some(SessionRef {
        id,
        title,
        updated_at,
    })
}

/// Read just enough of a file to grab the first user message as a title.
/// Bounded: stops at the first user turn, skips malformed lines.
fn first_user_snippet(file: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(file).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(turn) = turn_from_value(&value) {
            if turn.role == Role::User && !turn.text.trim().is_empty() {
                return Some(snippet(&turn.text, TITLE_SNIPPET_CHARS));
            }
        }
    }
    None
}

/// Map one JSON event to a [`Turn`], tolerant of Claude and Codex shapes.
///
/// Recognized shapes (first match wins):
/// - Claude: `{"type":"user"|"assistant", "message":{"role":_, "content":_}}`
/// - Generic: `{"role":_, "content":_}` or `{"role":_, "text":_}`
/// - Codex rollout: `{"type":"message", "role":_, "content":_}`
///
/// Returns `None` for events with no usable role+text (tool calls, summaries,
/// system metadata) so they are silently skipped.
fn turn_from_value(value: &Value) -> Option<Turn> {
    // Prefer a nested `message` object (Claude), else the top level.
    let msg = value.get("message").unwrap_or(value);

    let role_str = msg
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))?;
    let role = match role_str {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => Role::Other,
    };

    let text = extract_text(msg).or_else(|| extract_text(value))?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Turn { role, text })
}

/// Extract human-readable text from a message object's `content`/`text` field.
///
/// `content` may be a plain string or an array of content blocks (Anthropic
/// shape: `[{"type":"text","text":"…"}, …]`). Non-text blocks are ignored.
fn extract_text(msg: &Value) -> Option<String> {
    if let Some(s) = msg.get("text").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    match msg.get("content") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                if let Some(s) = block.get("text").and_then(Value::as_str) {
                    if !s.is_empty() {
                        parts.push(s.to_string());
                    }
                } else if let Some(s) = block.as_str() {
                    if !s.is_empty() {
                        parts.push(s.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionFormat;
    use std::io::Write;

    fn store_at(path: PathBuf) -> SessionStore {
        SessionStore {
            path,
            format: SessionFormat::Jsonl,
        }
    }

    #[test]
    fn read_parses_turns_skips_malformed_and_bounds_max_turns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sess-abc.jsonl");
        let mut f = std::fs::File::create(&file).expect("create");
        // Claude-style nested message, string content.
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"Hello there"}}}}"#
        )
        .unwrap();
        // Anthropic content-block array.
        writeln!(
            f,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Hi back"}}]}}}}"#
        )
        .unwrap();
        // Malformed line — must be skipped, not panic.
        writeln!(f, r#"{{not valid json,,,"#).unwrap();
        // Generic top-level shape.
        writeln!(f, r#"{{"role":"user","text":"Third turn"}}"#).unwrap();
        // A tool/system event with no usable text — skipped.
        writeln!(f, r#"{{"type":"tool_use","id":"t1"}}"#).unwrap();
        drop(f);

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());

        let t = reader.read(&store, "sess-abc", 10).expect("read");
        assert_eq!(t.turns.len(), 3, "malformed + textless lines skipped");
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "Hello there");
        assert_eq!(t.turns[1].role, Role::Assistant);
        assert_eq!(t.turns[1].text, "Hi back");
        assert_eq!(t.turns[2].text, "Third turn");

        // max_turns bound.
        let bounded = reader.read(&store, "sess-abc", 2).expect("read bounded");
        assert_eq!(bounded.turns.len(), 2);
    }

    #[test]
    fn list_enumerates_files_and_extracts_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sess-1.jsonl");
        let mut f = std::fs::File::create(&file).expect("create");
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"What is the auth flow?"}}}}"#
        )
        .unwrap();
        drop(f);

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, "sess-1");
        assert_eq!(refs[0].title.as_deref(), Some("What is the auth flow?"));
        assert!(refs[0].updated_at.parse::<u64>().is_ok());
    }

    #[test]
    fn list_respects_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..5 {
            let file = dir.path().join(format!("s{i}.jsonl"));
            let mut f = std::fs::File::create(&file).expect("create");
            writeln!(f, r#"{{"role":"user","content":"hi {i}"}}"#).unwrap();
        }
        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 2).expect("list");
        assert_eq!(refs.len(), 2);
    }
}
