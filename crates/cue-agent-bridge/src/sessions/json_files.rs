//! VS Code / Copilot chat-session decoder (one JSON file per session).
//!
//! Storage layout (design §3):
//! `…/User/workspaceStorage/<hash>/chatSessions/<id>.json` (and possibly under
//! `globalStorage`). Each file is one session containing a `requests` (or
//! `messages`) array.
//!
//! These are strict JSON (not JSONC), but parsing is still defensive: a file
//! that fails to parse is skipped in `list`, and unrecognized entries within a
//! file are skipped in `read`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{mtime_epoch_string, snippet, SessionReader};
use crate::{Role, SessionRef, SessionStore, Transcript, Turn};

/// Decoder for VS Code / Copilot `chatSessions/*.json` files.
pub struct JsonFilesReader;

const TITLE_SNIPPET_CHARS: usize = 80;

impl SessionReader for JsonFilesReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let mut refs: Vec<SessionRef> = enumerate_files(&store.path)
            .into_iter()
            .filter_map(|file| session_ref_for(&file))
            .collect();
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
        // JSON requires a full parse (no cheap streaming of the `requests`
        // array), and these files reach hundreds of MB. Cap the parse so
        // selecting a giant session never hangs — above the cap, surface one
        // honest turn rather than spending many seconds parsing.
        if std::fs::metadata(&file)
            .map(|m| m.len() > MAX_READ_PARSE_BYTES)
            .unwrap_or(false)
        {
            turns.push(Turn {
                role: Role::System,
                text: "This session is too large to load fully here. Open it in \
                       the app, or ask a focused question."
                    .to_string(),
            });
            return Ok(Transcript { turns });
        }
        let contents = std::fs::read_to_string(&file)
            .map_err(|_| crate::BridgeError::Unreadable(file.clone()))?;
        let root: Value = serde_json::from_str(&contents)
            .map_err(|e| crate::BridgeError::Parse(e.to_string()))?;
        for entry in message_entries(&root) {
            if let Some(turn) = entry_to_turn(entry) {
                turns.push(turn);
                if turns.len() >= max_turns {
                    break;
                }
            }
        }
        Ok(Transcript { turns })
    }
}

/// Enumerate `*.json` chat-session files under a store path.
///
/// VS Code / Copilot store sessions at
/// `…/User/workspaceStorage/<hash>/chatSessions/<id>.json`, so when `path` is
/// the `workspaceStorage` root we must descend two levels: into each
/// `<hash>/` workspace dir and its `chatSessions/` subdir. We also accept a
/// path that already points directly at a `chatSessions/` dir or a single file,
/// and any direct `*.json` children, so the reader works regardless of which
/// level the store path names.
fn enumerate_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut files = Vec::new();
    collect_json(path, &mut files);
    // Descend into `<hash>/chatSessions/` (workspaceStorage layout).
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            // Either this IS a chatSessions dir, or it's a <hash> dir holding one.
            if p.file_name().is_some_and(|n| n == "chatSessions") {
                collect_json(&p, &mut files);
            } else {
                collect_json(&p.join("chatSessions"), &mut files);
            }
        }
    }
    files
}

/// Append every direct `*.json` child of `dir` to `out` (no recursion).
fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e == "json") {
                out.push(p);
            }
        }
    }
}

/// Resolve a session `id` back to its `*.json` file, searching the same
/// locations [`enumerate_files`] scans.
fn resolve_file(path: &Path, id: &str) -> PathBuf {
    if path.is_file() {
        return path.to_path_buf();
    }
    let target = format!("{id}.json");
    let direct = path.join(&target);
    if direct.is_file() {
        return direct;
    }
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let candidates = [p.join(&target), p.join("chatSessions").join(&target)];
            for c in candidates {
                if c.is_file() {
                    return c;
                }
            }
        }
    }
    direct
}

fn session_ref_for(file: &Path) -> Option<SessionRef> {
    let id = file.file_stem()?.to_string_lossy().into_owned();
    let updated_at = mtime_epoch_string(file);

    // Skip EMPTY sessions (VS Code/Copilot auto-create chat files with zero
    // requests). A large file definitely has content (and we won't parse it
    // for an emptiness check — too slow); a small file is cheap to check.
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size <= MAX_TITLE_PARSE_BYTES && !has_messages(file) {
        return None;
    }

    // Title: cleaned first user message, or `None` (no readable topic). The
    // reader stays agent-agnostic — a generic, registry-driven fallback label
    // is applied by the caller, not hardcoded per reader.
    let title = super::clean_title(first_user_snippet(file), TITLE_SNIPPET_CHARS);
    Some(SessionRef {
        id,
        title,
        updated_at,
        project: None,
    })
}

/// Cheap check: does this (small) session file contain at least one message?
/// Used to drop empty auto-created chat files from the listing.
fn has_messages(file: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(file) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };
    !message_entries(&root).is_empty()
}

/// Max file size we will read+parse just to extract a title during `list`.
/// VS Code chat-session JSON files can be **hundreds of MB** (observed: 141 MB),
/// and `list` titles every session — parsing a giant file per row made listing
/// take ~24s. Above this cap we skip the title (the row still lists, with no
/// title); the full content is still readable on demand in `read`, bounded by
/// `max_turns`.
const MAX_TITLE_PARSE_BYTES: u64 = 2 * 1024 * 1024;

/// Max file size we will fully parse on an explicit `read` (session selected).
/// Reading is intentional so the cap is higher than the title cap, but still
/// bounded — a 141 MB chat file would otherwise take many seconds to parse.
const MAX_READ_PARSE_BYTES: u64 = 25 * 1024 * 1024;

fn first_user_snippet(file: &Path) -> Option<String> {
    // Skip titling oversized files — never parse a multi-hundred-MB JSON just
    // for a title. Listing must stay fast.
    let too_big = std::fs::metadata(file)
        .map(|m| m.len() > MAX_TITLE_PARSE_BYTES)
        .unwrap_or(true);
    if too_big {
        return None;
    }
    let contents = std::fs::read_to_string(file).ok()?;
    let root: Value = serde_json::from_str(&contents).ok()?;
    for entry in message_entries(&root) {
        if let Some(turn) = entry_to_turn(entry) {
            if turn.role == Role::User && !turn.text.trim().is_empty() {
                return Some(snippet(&turn.text, TITLE_SNIPPET_CHARS));
            }
        }
    }
    None
}

/// Locate the message array in a session document, tolerant of the two known
/// VS Code shapes: a top-level `requests` array or a `messages` array.
fn message_entries(root: &Value) -> &[Value] {
    for field in ["requests", "messages"] {
        if let Some(Value::Array(arr)) = root.get(field) {
            return arr;
        }
    }
    &[]
}

/// Map a VS Code chat entry to a [`Turn`].
///
/// VS Code request entries pair a user `message` with a `response`; we emit the
/// user turn from `message.text`/`message` and, when present, an assistant turn
/// is handled by the caller iterating — here we return the most salient single
/// turn. To keep both sides, request entries expand to a user turn; assistant
/// text is appended via `response`. We therefore return the user turn and let
/// `read` also pick up responses through a second pass embedded here.
fn entry_to_turn(entry: &Value) -> Option<Turn> {
    // Generic `{role, content/text}` shape (also covers `messages` arrays).
    if let Some(role_str) = entry.get("role").and_then(Value::as_str) {
        let role = role_from_str(role_str);
        if let Some(text) = field_text(entry) {
            let text = text.trim().to_string();
            if !text.is_empty() {
                return Some(Turn { role, text });
            }
        }
    }
    // VS Code `requests` shape: a user message lives under `message`.
    if let Some(msg) = entry.get("message") {
        if let Some(text) = field_text(msg).or_else(|| msg.as_str().map(str::to_string)) {
            let text = text.trim().to_string();
            if !text.is_empty() {
                return Some(Turn {
                    role: Role::User,
                    text,
                });
            }
        }
    }
    None
}

fn role_from_str(s: &str) -> Role {
    match s {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        _ => Role::Other,
    }
}

/// Extract `text`/`content` text from a value (string or content-block array).
fn field_text(value: &Value) -> Option<String> {
    if let Some(s) = value.get("text").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    match value.get("content") {
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
            format: SessionFormat::JsonFiles,
        }
    }

    #[test]
    fn read_parses_messages_array() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sess-x.json");
        let mut f = std::fs::File::create(&file).expect("create");
        let doc = r#"{
            "version": 3,
            "messages": [
                {"role":"user","text":"How do I run tests?"},
                {"role":"assistant","content":"Use cargo test."},
                {"role":"assistant"}
            ]
        }"#;
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let t = reader.read(&store, "sess-x", 10).expect("read");
        assert_eq!(t.turns.len(), 2, "textless entry skipped");
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "How do I run tests?");
        assert_eq!(t.turns[1].role, Role::Assistant);
        assert_eq!(t.turns[1].text, "Use cargo test.");
    }

    #[test]
    fn read_parses_requests_message_shape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("req.json");
        let mut f = std::fs::File::create(&file).expect("create");
        let doc = r#"{
            "requests": [
                {"message":{"text":"first user prompt"}},
                {"message":"plain string prompt"}
            ]
        }"#;
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let t = reader.read(&store, "req", 10).expect("read");
        assert_eq!(t.turns.len(), 2);
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "first user prompt");
        assert_eq!(t.turns[1].text, "plain string prompt");
    }

    #[test]
    fn list_enumerates_and_titles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("s1.json");
        let mut f = std::fs::File::create(&file).expect("create");
        f.write_all(br#"{"messages":[{"role":"user","text":"hello world"}]}"#)
            .unwrap();
        drop(f);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, "s1");
        assert_eq!(refs[0].title.as_deref(), Some("hello world"));
    }
}
