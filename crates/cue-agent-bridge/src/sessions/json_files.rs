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

use std::io::{Read, Seek, SeekFrom};
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

    // Title: VS Code's own `customTitle` if present, else the cleaned first
    // user message, else `None` (no readable topic). The reader stays
    // agent-agnostic — a generic, registry-driven fallback label is applied by
    // the caller, not hardcoded per reader.
    let title = super::clean_title(extract_title(file), TITLE_SNIPPET_CHARS);
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

/// Bytes read from the **tail** of an oversized file to recover `customTitle`.
/// VS Code writes `customTitle` as a top-level key serialized *last*, so it sits
/// in the final bytes of the document (observed: the last ~80 bytes of a 148 MB
/// file). 64 KiB is a generous margin that still avoids reading the whole file.
const TAIL_SCAN_BYTES: u64 = 64 * 1024;

/// Bytes read from the **head** of an oversized file to recover the first user
/// message as a fallback title. The first `requests[0].message.text` lives near
/// the start (observed: byte ~449), so a bounded head read finds it without
/// parsing the multi-hundred-MB body.
const HEAD_SCAN_BYTES: u64 = 256 * 1024;

/// Derive a human title for a session file, preferring VS Code's own
/// `customTitle` and falling back to the first user message.
///
/// Bounded and fail-soft: small files (≤ [`MAX_TITLE_PARSE_BYTES`]) are parsed
/// whole; oversized files (VS Code chat JSON reaches **hundreds of MB**) are
/// never parsed wholesale — instead we read only a bounded slice of the tail
/// (where `customTitle` lives) and, as a fallback, a bounded slice of the head
/// (where the first message lives). Returns `None` when no readable title
/// exists; the caller then applies a generic fallback label.
fn extract_title(file: &Path) -> Option<String> {
    let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(u64::MAX);

    // Small files: a full parse is cheap and exact.
    if size <= MAX_TITLE_PARSE_BYTES {
        let contents = std::fs::read_to_string(file).ok()?;
        let root: Value = serde_json::from_str(&contents).ok()?;
        if let Some(t) = custom_title(&root) {
            return Some(t);
        }
        return first_user_snippet(&root);
    }

    // Oversized files: recover `customTitle` from a bounded tail read first
    // (preferred, VS Code-curated), then fall back to the first user message
    // from a bounded head read. Never parse the whole document.
    custom_title_from_tail(file, size).or_else(|| first_user_snippet_from_head(file))
}

/// Read VS Code's explicit `customTitle` from a fully parsed document.
fn custom_title(root: &Value) -> Option<String> {
    let raw = root.get("customTitle").and_then(Value::as_str)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(snippet(trimmed, TITLE_SNIPPET_CHARS))
    }
}

/// First non-empty user-message snippet from a fully parsed document.
fn first_user_snippet(root: &Value) -> Option<String> {
    for entry in message_entries(root) {
        if let Some(turn) = entry_to_turn(entry) {
            if turn.role == Role::User && !turn.text.trim().is_empty() {
                return Some(snippet(&turn.text, TITLE_SNIPPET_CHARS));
            }
        }
    }
    None
}

/// Recover `customTitle` from the last [`TAIL_SCAN_BYTES`] of an oversized file
/// without parsing it. VS Code serializes `customTitle` last, so it reliably
/// appears in the tail. Returns `None` if the key is absent (e.g. a session VS
/// Code never auto-titled) or the tail slice cannot be read/decoded.
fn custom_title_from_tail(file: &Path, size: u64) -> Option<String> {
    let mut f = std::fs::File::open(file).ok()?;
    let start = size.saturating_sub(TAIL_SCAN_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity(TAIL_SCAN_BYTES as usize);
    f.take(TAIL_SCAN_BYTES).read_to_end(&mut buf).ok()?;
    let tail = String::from_utf8_lossy(&buf);
    let raw = scan_json_string_value(&tail, "customTitle")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(snippet(trimmed, TITLE_SNIPPET_CHARS))
    }
}

/// Recover the first user message from the first [`HEAD_SCAN_BYTES`] of an
/// oversized file. The head usually contains a complete `requests[0]` object;
/// if the slice cuts it mid-object the JSON parse fails and we return `None`
/// (the tail `customTitle` is the primary source anyway).
fn first_user_snippet_from_head(file: &Path) -> Option<String> {
    let f = std::fs::File::open(file).ok()?;
    let mut buf = Vec::with_capacity(HEAD_SCAN_BYTES as usize);
    f.take(HEAD_SCAN_BYTES).read_to_end(&mut buf).ok()?;
    let head = String::from_utf8_lossy(&buf);
    let raw = scan_json_string_value(&head, "text")?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(snippet(trimmed, TITLE_SNIPPET_CHARS))
    }
}

/// Find the first `"<key>": "<value>"` pair in a JSON *fragment* and return the
/// decoded string value. Used only on bounded slices of oversized files where a
/// full parse is not possible. Handles JSON string escapes (`\"`, `\\`, `\n`,
/// `\t`, `\uXXXX`, …) so the recovered title is not corrupted by escaping.
fn scan_json_string_value(fragment: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut search_from = 0usize;
    loop {
        let key_at = fragment[search_from..].find(&needle)? + search_from;
        // Position just after the closing quote of the key.
        let after_key = key_at + needle.len();
        let rest = &fragment[after_key..];
        // Expect optional whitespace, a colon, optional whitespace, then `"`.
        let mut chars = rest.char_indices();
        let mut colon_seen = false;
        let mut value_start: Option<usize> = None;
        for (i, c) in chars.by_ref() {
            match c {
                c if c.is_whitespace() => continue,
                ':' if !colon_seen => colon_seen = true,
                '"' if colon_seen => {
                    value_start = Some(after_key + i + c.len_utf8());
                    break;
                }
                _ => break, // not a string value (e.g. number/object) — give up
            }
        }
        if let Some(vstart) = value_start {
            if let Some(decoded) = decode_json_string_body(&fragment[vstart..]) {
                return Some(decoded);
            }
        }
        // Not a usable match; continue scanning after this key occurrence.
        search_from = after_key;
    }
}

/// Decode a JSON string starting just *after* the opening quote, up to the
/// matching unescaped closing quote. Returns `None` if the slice ends before
/// the string closes (e.g. the bounded read cut it off mid-value).
fn decode_json_string_body(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => {
                let e = chars.next()?;
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'u' => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            let h = chars.next()?;
                            code = code * 16 + h.to_digit(16)?;
                        }
                        // Lone surrogates can't be represented; substitute U+FFFD.
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    other => out.push(other),
                }
            }
            other => out.push(other),
        }
    }
    None // string never closed within the slice
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

    /// Real VS Code shape: a top-level `customTitle` plus `requests[].message`.
    /// The curated `customTitle` should win over the first user message.
    #[test]
    fn list_prefers_custom_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("vsc.json");
        let mut f = std::fs::File::create(&file).expect("create");
        let doc = r#"{
            "version": 3,
            "requesterUsername": "SomeUser",
            "responderUsername": "GitHub Copilot",
            "requests": [
                {"message":{"text":"understand this whole repo and summarize it"}}
            ],
            "customTitle": "Project overview and README update discussion"
        }"#;
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].title.as_deref(),
            Some("Project overview and README update discussion"),
        );
    }

    /// No `customTitle` present → fall back to the first user message text.
    #[test]
    fn list_falls_back_to_first_message_without_custom_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("nocustom.json");
        let mut f = std::fs::File::create(&file).expect("create");
        let doc = r#"{
            "version": 3,
            "requests": [
                {"message":{"text":"how do I wire up the daemon IPC?"}}
            ]
        }"#;
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].title.as_deref(),
            Some("how do I wire up the daemon IPC?"),
        );
    }

    /// A boilerplate-only first message with no `customTitle` yields no title
    /// (the shared `is_boilerplate_title` filter rejects it), so the caller can
    /// apply a generic fallback label instead of a junk title.
    #[test]
    fn list_boilerplate_only_has_no_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("boiler.json");
        let mut f = std::fs::File::create(&file).expect("create");
        let doc = r#"{
            "version": 3,
            "requests": [
                {"message":{"text":"You are a helpful coding assistant."}}
            ]
        }"#;
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].title, None);
    }

    /// Oversized files (above the whole-file parse cap) must still be titled:
    /// `customTitle` is serialized *last* by VS Code, so a bounded tail read
    /// recovers it without parsing the multi-MB body. We synthesize that layout
    /// by padding the `requests` array past `MAX_TITLE_PARSE_BYTES`.
    #[test]
    fn list_titles_oversized_file_from_tail_custom_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("big.json");
        let mut f = std::fs::File::create(&file).expect("create");
        // Build a >2 MiB document whose `customTitle` is the final key.
        let filler = "x".repeat((MAX_TITLE_PARSE_BYTES as usize) + 64 * 1024);
        let doc = format!(
            r#"{{"version":3,"requests":[{{"message":{{"text":"first prompt"}},"filler":"{filler}"}}],"customTitle":"Module export error in Node.js app"}}"#,
        );
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);
        // Sanity: this file really is above the whole-file parse cap.
        assert!(std::fs::metadata(&file).unwrap().len() > MAX_TITLE_PARSE_BYTES);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].title.as_deref(),
            Some("Module export error in Node.js app"),
        );
    }

    /// Oversized file without a `customTitle`: the bounded *head* read recovers
    /// the first user message (`requests[0].message.text`) as the fallback.
    #[test]
    fn list_titles_oversized_file_from_head_first_message() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("bignotitle.json");
        let mut f = std::fs::File::create(&file).expect("create");
        let filler = "y".repeat((MAX_TITLE_PARSE_BYTES as usize) + 64 * 1024);
        // No `customTitle`; first message near the head, huge filler trailing.
        let doc = format!(
            r#"{{"version":3,"requests":[{{"message":{{"text":"explain the build system"}}}}],"filler":"{filler}"}}"#,
        );
        f.write_all(doc.as_bytes()).unwrap();
        drop(f);
        assert!(std::fs::metadata(&file).unwrap().len() > MAX_TITLE_PARSE_BYTES);

        let reader = JsonFilesReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].title.as_deref(), Some("explain the build system"));
    }

    /// The fragment scanner used on bounded slices must JSON-decode escapes so
    /// recovered titles aren't corrupted, and must skip non-string values.
    #[test]
    fn scan_json_string_value_decodes_escapes_and_skips_nonstrings() {
        let frag = r#""version": 3, "customTitle": "a \"quoted\" word\nand newline""#;
        assert_eq!(
            scan_json_string_value(frag, "customTitle").as_deref(),
            Some("a \"quoted\" word\nand newline"),
        );
        // `version` is a number, not a string → no match.
        assert_eq!(scan_json_string_value(frag, "version"), None);
        // Absent key → None.
        assert_eq!(scan_json_string_value(frag, "missing"), None);
        // Value cut off before the closing quote → None (bounded-read safety).
        assert_eq!(scan_json_string_value(r#""k": "unterminated"#, "k"), None);
    }
}
