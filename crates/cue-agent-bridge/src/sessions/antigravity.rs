//! Antigravity (Google) session reader.
//!
//! Antigravity stores conversations under `~/.gemini/antigravity/`:
//! - `agyhub_summaries_proto.pb` — a **plaintext protobuf INDEX** of every
//!   conversation: `(uuid, title, workspace/project)` as sibling fields. This is
//!   what the Antigravity desktop UI itself lists from (the UI does not scan
//!   `conversations/` directly). Wire format byte-verified on a real machine —
//!   see [`parse_index`].
//! - `conversations/<uuid>.pb` — per-conversation BODIES, **encrypted** via
//!   Electron `safeStorage` (macOS Keychain, hardware-bound). Not decodable.
//! - `conversations/<uuid>.db` — newer per-conversation bodies as **plaintext
//!   SQLite** (`steps` table). Readable.
//! - `brain/<uuid>/.system_generated/logs/transcript.jsonl` — readable JSONL
//!   agent logs, present for a subset of conversations.
//!
//! ### Strategy (a superset of the old brain-only reader)
//! - [`list`](AntigravityReader::list): parse the plaintext **index** → all
//!   conversations, with their real titles and projects. This replaces the old
//!   approach that read only the ~6 `brain/` transcripts and missed ~94%.
//! - [`read`](AntigravityReader::read): prefer the richest **readable** body for
//!   a `uuid`, in order: the `brain/` JSONL transcript (most complete) →
//!   the `conversations/<uuid>.db` SQLite body. An encrypted `.pb`-only
//!   conversation has no readable body, so `read` returns an empty transcript
//!   (it is still *listed*, with title+project, from the index).
//!
//! Security invariants match every other reader: read-only, bounded, fail-soft,
//! no secrets (only titles, project paths, and conversation text). The index is
//! parsed with a tiny hand-rolled varint reader — **no protobuf crate
//! dependency** — and only string fields at known tags are extracted; unknown
//! fields are skipped generically.

use std::path::{Path, PathBuf};

use super::{mtime_epoch_string, SessionReader};
use crate::{Role, SessionRef, SessionStore, Transcript, Turn};

/// Reader for [`SessionFormat::AntigravityIndex`](crate::SessionFormat::AntigravityIndex).
///
/// `SessionStore::path` points at the **index file** itself
/// (`<data_dir>/agyhub_summaries_proto.pb`), NOT the data-dir root — so this
/// reader never enumerates the credential-bearing root. The store root (for
/// resolving `conversations/` and `brain/` bodies) is the index file's parent.
pub struct AntigravityReader;

/// Cap on the index file size we will read whole (it is ~80 KB in practice).
const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;
/// Cap chars kept for a derived title (titles can be a whole first message).
const TITLE_MAX_CHARS: usize = 90;

impl SessionReader for AntigravityReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let index = &store.path;
        let root = index.parent().unwrap_or(index);
        let summaries = parse_index(index);

        let mut refs: Vec<SessionRef> = summaries
            .into_iter()
            .map(|s| {
                // Recency: mtime of the conversation body if present, else the
                // index file's mtime (best-effort; the index has no per-row
                // timestamp we rely on — the protobuf timestamps are unlabeled).
                let updated_at = body_path(root, &s.uuid)
                    .map(|p| mtime_epoch_string(&p))
                    .unwrap_or_else(|| mtime_epoch_string(index));
                let project = s.project.clone();
                let title = super::clean_title(Some(s.title), TITLE_MAX_CHARS)
                    .or_else(|| super::fallback_label(project.as_deref(), &updated_at));
                SessionRef {
                    id: s.uuid,
                    title,
                    updated_at,
                    project,
                }
            })
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
        if max_turns == 0 {
            return Ok(Transcript { turns: Vec::new() });
        }
        let root = store.path.parent().unwrap_or(&store.path);
        // Prefer the brain JSONL transcript (richest readable body) when present.
        let brain = root
            .join("brain")
            .join(id)
            .join(".system_generated")
            .join("logs")
            .join("transcript.jsonl");
        if brain.is_file() {
            return super::jsonl::read_transcript_file(&brain, max_turns);
        }
        // Else the newer SQLite body, if this conversation has one.
        let db = root.join("conversations").join(format!("{id}.db"));
        if db.is_file() {
            return Ok(read_db_body(&db, max_turns));
        }
        // Encrypted `.pb`-only conversation: listed (with title+project) but no
        // readable body. Return empty rather than erroring.
        Ok(Transcript { turns: Vec::new() })
    }
}

/// The on-disk body path for a conversation `uuid`, if any readable/known body
/// exists (`.db` preferred for recency; `.pb` is the encrypted fallback that at
/// least gives an mtime). `None` if neither exists.
fn body_path(root: &Path, uuid: &str) -> Option<PathBuf> {
    let db = root.join("conversations").join(format!("{uuid}.db"));
    if db.is_file() {
        return Some(db);
    }
    let pb = root.join("conversations").join(format!("{uuid}.pb"));
    if pb.is_file() {
        return Some(pb);
    }
    None
}

/// One conversation row extracted from the index.
struct Summary {
    uuid: String,
    title: String,
    project: Option<String>,
}

// ---------------------------------------------------------------------------
// Minimal protobuf wire reader (no protobuf crate).
//
// Wire format (verified): the index is `repeated ConversationSummary` at field
// 1. Each `ConversationSummary` = { field 1: uuid (string), field 2: Summary }.
// `Summary` = { field 1: title (string), field 9: Workspace (repeated) , … }.
// `Workspace` = { field 1: uri (string), … }. We extract uuid + title + the
// first workspace uri; every other field is skipped generically by wire type.
// ---------------------------------------------------------------------------

/// A cursor over protobuf bytes with fail-soft varint/field reads.
struct Buf<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Buf<'a> {
    fn new(b: &'a [u8]) -> Self {
        Buf { b, i: 0 }
    }
    fn done(&self) -> bool {
        self.i >= self.b.len()
    }
    /// Read a base-128 varint. `None` on truncation/overlong.
    fn varint(&mut self) -> Option<u64> {
        let mut val: u64 = 0;
        let mut shift = 0u32;
        loop {
            if self.i >= self.b.len() || shift >= 64 {
                return None;
            }
            let byte = self.b[self.i];
            self.i += 1;
            val |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return Some(val);
            }
            shift += 7;
        }
    }
    /// Read a length-delimited byte slice (wire type 2). `None` if it would run
    /// past the buffer.
    fn len_delim(&mut self) -> Option<&'a [u8]> {
        let len = self.varint()? as usize;
        let end = self.i.checked_add(len)?;
        if end > self.b.len() {
            return None;
        }
        let s = &self.b[self.i..end];
        self.i = end;
        Some(s)
    }
    /// Skip a field's payload given its wire type. Returns `false` on a wire type
    /// we don't handle (groups) or on truncation, so the caller can stop.
    fn skip(&mut self, wire: u64) -> bool {
        match wire {
            0 => self.varint().is_some(),    // varint
            1 => self.advance(8),            // 64-bit
            2 => self.len_delim().is_some(), // length-delimited
            5 => self.advance(4),            // 32-bit
            _ => false,                      // 3/4 groups — unexpected
        }
    }
    fn advance(&mut self, n: usize) -> bool {
        match self.i.checked_add(n) {
            Some(end) if end <= self.b.len() => {
                self.i = end;
                true
            }
            _ => false,
        }
    }
    /// Read the next `(field_number, wire_type)` tag. `None` at clean EOF.
    fn tag(&mut self) -> Option<(u64, u64)> {
        if self.done() {
            return None;
        }
        let t = self.varint()?;
        Some((t >> 3, t & 7))
    }
}

/// Parse the plaintext index file into conversation summaries. Fail-soft: an
/// unreadable/oversized/garbled file yields an empty list (the caller then
/// simply lists nothing for Antigravity, never errors).
fn parse_index(path: &Path) -> Vec<Summary> {
    let mut out = Vec::new();
    let Ok(meta) = std::fs::metadata(path) else {
        return out;
    };
    if meta.len() > MAX_INDEX_BYTES {
        return out;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return out;
    };
    let mut buf = Buf::new(&bytes);
    // Top-level: repeated ConversationSummary at field 1 (wire 2). Tolerate
    // other top-level fields by skipping them.
    while let Some((field, wire)) = buf.tag() {
        if field == 1 && wire == 2 {
            if let Some(msg) = buf.len_delim() {
                if let Some(s) = parse_conversation(msg) {
                    out.push(s);
                }
            } else {
                break;
            }
        } else if !buf.skip(wire) {
            break;
        }
    }
    out
}

/// Parse one `ConversationSummary` message: field 1 = uuid, field 2 = Summary.
fn parse_conversation(bytes: &[u8]) -> Option<Summary> {
    let mut buf = Buf::new(bytes);
    let mut uuid: Option<String> = None;
    let mut title: Option<String> = None;
    let mut project: Option<String> = None;
    while let Some((field, wire)) = buf.tag() {
        match (field, wire) {
            (1, 2) => {
                uuid = buf.len_delim().map(lossy);
            }
            (2, 2) => {
                if let Some(inner) = buf.len_delim() {
                    let (t, p) = parse_summary(inner);
                    title = t;
                    project = p;
                }
            }
            (_, w) => {
                if !buf.skip(w) {
                    break;
                }
            }
        }
    }
    let uuid = uuid?;
    // A valid row needs the uuid; title may be absent (fall back later).
    Some(Summary {
        uuid,
        title: title.unwrap_or_default(),
        project,
    })
}

/// Parse the inner `Summary`: field 1 = title, field 9 = Workspace (repeated;
/// take the first workspace's uri, field 1, as the project). Other fields
/// skipped.
fn parse_summary(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let mut buf = Buf::new(bytes);
    let mut title: Option<String> = None;
    let mut project: Option<String> = None;
    while let Some((field, wire)) = buf.tag() {
        match (field, wire) {
            (1, 2) => {
                title = buf.len_delim().map(lossy);
            }
            (9, 2) => {
                // First workspace wins for the project label.
                if project.is_none() {
                    if let Some(ws) = buf.len_delim() {
                        project = parse_workspace_uri(ws);
                    }
                } else if !buf.skip(wire) {
                    break;
                }
            }
            (_, w) => {
                if !buf.skip(w) {
                    break;
                }
            }
        }
    }
    (title, project)
}

/// Parse a `Workspace` message and return its uri (field 1), stripped of the
/// `file://` scheme so it reads as a plain path. `None` if absent/empty.
fn parse_workspace_uri(bytes: &[u8]) -> Option<String> {
    let mut buf = Buf::new(bytes);
    while let Some((field, wire)) = buf.tag() {
        if field == 1 && wire == 2 {
            let uri = buf.len_delim().map(lossy)?;
            let uri = uri.trim();
            if uri.is_empty() {
                return None;
            }
            return Some(strip_file_scheme(uri));
        } else if !buf.skip(wire) {
            break;
        }
    }
    None
}

/// `file:///Users/me/x` → `/Users/me/x`; leaves non-file URIs/paths untouched.
fn strip_file_scheme(uri: &str) -> String {
    // Strip the scheme AND percent-decode, so a project path like
    // `file:///Users/ms/GEMMA4%20Vision%20test` becomes `/Users/ms/GEMMA4 Vision
    // test` instead of leaving raw `%20` mojibake in the displayed project.
    let stripped = uri.strip_prefix("file://").unwrap_or(uri);
    super::percent_decode(stripped)
}

/// Lossy-UTF8 a byte slice into an owned String (index strings are UTF-8 but we
/// never panic on a stray byte).
fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

// ---------------------------------------------------------------------------
// SQLite body reader for `conversations/<uuid>.db` (newer Antigravity format).
// ---------------------------------------------------------------------------

/// Read turns from a conversation `.db`. The `steps` table holds per-turn rows;
/// `step_payload` carries the text and the turn role marker. Read-only,
/// `immutable=1`, bounded to `max_turns`, fail-soft (any error → empty).
fn read_db_body(db: &Path, max_turns: usize) -> Transcript {
    use rusqlite::{Connection, OpenFlags};
    let mut turns = Vec::new();
    let path = db.to_string_lossy();
    let uri = format!("file:{path}?immutable=1&mode=ro");
    let Ok(conn) = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return Transcript { turns };
    };
    // `steps` rows are ordered; `step_payload` is JSON/text with the message.
    // We pull payloads in row order and extract human text + a role guess.
    let Ok(mut stmt) = conn.prepare("SELECT step_payload FROM steps ORDER BY rowid LIMIT ?1")
    else {
        return Transcript { turns };
    };
    let cap = max_turns.saturating_mul(4).max(max_turns) as i64;
    let rows = stmt.query_map([cap], |row| {
        // Payload may be stored as TEXT or BLOB; accept either.
        let payload: Option<String> = row
            .get::<_, Option<String>>(0)
            .or_else(|_| {
                row.get::<_, Option<Vec<u8>>>(0)
                    .map(|o| o.map(|b| String::from_utf8_lossy(&b).into_owned()))
            })
            .unwrap_or(None);
        Ok(payload)
    });
    let Ok(rows) = rows else {
        return Transcript { turns };
    };
    for payload in rows.flatten().flatten() {
        if let Some(turn) = turn_from_step_payload(&payload) {
            turns.push(turn);
            if turns.len() >= max_turns {
                break;
            }
        }
    }
    Transcript { turns }
}

/// Best-effort extraction of a [`Turn`] from an Antigravity `steps.step_payload`.
/// The payload is JSON-ish with conversation text; we reuse the JSONL turn
/// decoder's shape tolerance by parsing it as a value and pulling text. A
/// payload with no usable text yields `None` (skipped).
fn turn_from_step_payload(payload: &str) -> Option<Turn> {
    let payload = payload.trim();
    if payload.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    // Role markers seen in payloads: "USER_INPUT" / "CONVERSATION_HISTORY" /
    // model/assistant. Look at a `step_type`/`type`/`role` field, then text.
    let role = role_from_value(&value);
    let text = text_from_value(&value)?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Turn { role, text })
}

fn role_from_value(value: &serde_json::Value) -> Role {
    for key in ["role", "step_type", "type"] {
        if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
            let u = s.to_ascii_uppercase();
            if u.contains("USER") {
                return Role::User;
            }
            if u.contains("MODEL") || u.contains("ASSISTANT") {
                return Role::Assistant;
            }
        }
    }
    Role::Other
}

/// Pull human text from a payload value: a `content`/`text`/`message` string, or
/// nested under those keys. Bounded shallow lookups; fail-soft.
fn text_from_value(value: &serde_json::Value) -> Option<String> {
    for key in ["content", "text", "message"] {
        match value.get(key) {
            Some(serde_json::Value::String(s)) if !s.is_empty() => return Some(s.clone()),
            Some(serde_json::Value::Object(_)) => {
                if let Some(inner) = value.get(key) {
                    if let Some(s) = inner.get("text").and_then(|v| v.as_str()) {
                        if !s.is_empty() {
                            return Some(s.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionFormat;

    #[test]
    fn strip_file_scheme_decodes_percent_escapes() {
        // C6b: a `file://` project URL with spaces must decode, not show mojibake.
        assert_eq!(
            strip_file_scheme("file:///Users/ms/Developer/GEMMA4%20Vision%20test%2026b%20MOE"),
            "/Users/ms/Developer/GEMMA4 Vision test 26b MOE"
        );
        // No scheme + no escapes → unchanged.
        assert_eq!(strip_file_scheme("/Users/ms/Bluey"), "/Users/ms/Bluey");
    }

    /// The store path is the INDEX FILE itself (mirrors `discover.rs`), so tests
    /// pass the data-dir root and we append the index filename here.
    fn store_at(root: PathBuf) -> SessionStore {
        SessionStore {
            path: root.join("agyhub_summaries_proto.pb"),
            format: SessionFormat::AntigravityIndex,
        }
    }

    // --- protobuf encoding helpers, to build a synthetic index in tests ---
    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut byte = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                break;
            }
        }
    }
    fn tag(field: u64, wire: u64, out: &mut Vec<u8>) {
        varint((field << 3) | wire, out);
    }
    fn len_delim(field: u64, bytes: &[u8], out: &mut Vec<u8>) {
        tag(field, 2, out);
        varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }
    fn workspace(uri: &str) -> Vec<u8> {
        let mut w = Vec::new();
        len_delim(1, uri.as_bytes(), &mut w); // field 1 = uri
        w
    }
    fn summary(title: &str, ws: Option<&str>) -> Vec<u8> {
        let mut s = Vec::new();
        len_delim(1, title.as_bytes(), &mut s); // field 1 = title
        tag(2, 0, &mut s); // field 2 = count (varint), exercise the skip path
        varint(7, &mut s);
        if let Some(uri) = ws {
            let w = workspace(uri);
            len_delim(9, &w, &mut s); // field 9 = workspace
        }
        s
    }
    fn conversation(uuid: &str, title: &str, ws: Option<&str>) -> Vec<u8> {
        let mut c = Vec::new();
        len_delim(1, uuid.as_bytes(), &mut c); // field 1 = uuid
        let s = summary(title, ws);
        len_delim(2, &s, &mut c); // field 2 = summary
        c
    }
    fn index(convs: &[(&str, &str, Option<&str>)]) -> Vec<u8> {
        let mut idx = Vec::new();
        for (uuid, title, ws) in convs {
            let c = conversation(uuid, title, *ws);
            len_delim(1, &c, &mut idx); // top-level field 1 = repeated conversation
        }
        idx
    }

    #[test]
    fn parse_index_extracts_uuid_title_project_with_reliable_pairing() {
        let bytes = index(&[
            (
                "0b65e315-32b5-4bd4-9e38-feb44aaac241",
                "Retrieving Pay Transparency Data",
                Some("file:///Users/ms/Desktop/Autoposting"),
            ),
            (
                "ab8c31cd-dbb6-4409-a0e3-d529d28e2ca0",
                "Understanding The Glyph Project",
                Some("file:///Users/ms/Desktop/Glyph"),
            ),
            // A conversation with no workspace (outside-of-project).
            (
                "d79f65a4-018e-483f-a03b-568200a18c70",
                "AI Recruiter Feature Development",
                None,
            ),
        ]);
        let summaries = parse_index_from_bytes(&bytes);
        assert_eq!(summaries.len(), 3);
        // Sibling-field pairing: each title stays with its own uuid + project.
        assert_eq!(summaries[0].uuid, "0b65e315-32b5-4bd4-9e38-feb44aaac241");
        assert_eq!(summaries[0].title, "Retrieving Pay Transparency Data");
        assert_eq!(
            summaries[0].project.as_deref(),
            Some("/Users/ms/Desktop/Autoposting"),
            "file:// scheme stripped"
        );
        assert_eq!(summaries[1].title, "Understanding The Glyph Project");
        assert_eq!(
            summaries[1].project.as_deref(),
            Some("/Users/ms/Desktop/Glyph")
        );
        assert_eq!(summaries[2].title, "AI Recruiter Feature Development");
        assert_eq!(summaries[2].project, None, "no workspace → no project");
    }

    /// Test helper: parse from an in-memory byte buffer (mirrors `parse_index`
    /// minus the file read).
    fn parse_index_from_bytes(bytes: &[u8]) -> Vec<Summary> {
        let mut out = Vec::new();
        let mut buf = Buf::new(bytes);
        while let Some((field, wire)) = buf.tag() {
            if field == 1 && wire == 2 {
                if let Some(msg) = buf.len_delim() {
                    if let Some(s) = parse_conversation(msg) {
                        out.push(s);
                    }
                } else {
                    break;
                }
            } else if !buf.skip(wire) {
                break;
            }
        }
        out
    }

    #[test]
    fn list_uses_index_and_falls_back_to_project_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let bytes = index(&[
            (
                "11111111-1111-1111-1111-111111111111",
                "Real Title Here",
                Some("file:///Users/ms/Developer/Bluey"),
            ),
            // Empty title → fall back to the project leaf ("Bluey session").
            (
                "22222222-2222-2222-2222-222222222222",
                "",
                Some("file:///x/Bluey"),
            ),
        ]);
        std::fs::write(root.join("agyhub_summaries_proto.pb"), &bytes).unwrap();

        let reader = AntigravityReader;
        let refs = reader
            .list(&store_at(root.to_path_buf()), 50)
            .expect("list");
        assert_eq!(refs.len(), 2);
        let real = refs
            .iter()
            .find(|r| r.id == "11111111-1111-1111-1111-111111111111")
            .unwrap();
        assert_eq!(real.title.as_deref(), Some("Real Title Here"));
        assert_eq!(real.project.as_deref(), Some("/Users/ms/Developer/Bluey"));
        let empty = refs
            .iter()
            .find(|r| r.id == "22222222-2222-2222-2222-222222222222")
            .unwrap();
        assert_eq!(empty.title.as_deref(), Some("Bluey session"));
    }

    #[test]
    fn list_missing_index_yields_empty_not_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reader = AntigravityReader;
        let refs = reader
            .list(&store_at(dir.path().to_path_buf()), 50)
            .expect("list must not error");
        assert!(refs.is_empty());
    }

    #[test]
    fn read_prefers_brain_transcript_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let id = "33333333-3333-3333-3333-333333333333";
        let brain = root
            .join("brain")
            .join(id)
            .join(".system_generated")
            .join("logs");
        std::fs::create_dir_all(&brain).unwrap();
        std::fs::write(
            brain.join("transcript.jsonl"),
            "{\"type\":\"user_input\",\"content\":\"hello antigravity\"}\n",
        )
        .unwrap();

        let reader = AntigravityReader;
        let t = reader
            .read(&store_at(root.to_path_buf()), id, 10)
            .expect("read");
        assert_eq!(t.turns.len(), 1);
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "hello antigravity");
    }

    #[test]
    fn read_encrypted_pb_only_conversation_is_empty_not_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let id = "44444444-4444-4444-4444-444444444444";
        let convs = root.join("conversations");
        std::fs::create_dir_all(&convs).unwrap();
        // An (encrypted) .pb body with no readable transcript.
        std::fs::write(convs.join(format!("{id}.pb")), b"\x00\x01\x02encrypted").unwrap();

        let reader = AntigravityReader;
        let t = reader
            .read(&store_at(root.to_path_buf()), id, 10)
            .expect("read must not error");
        assert!(t.turns.is_empty(), "encrypted body → empty, still listed");
    }

    #[test]
    fn varint_skip_tolerates_unknown_fields() {
        // A conversation carrying extra unknown fields (varint + 64-bit + 32-bit)
        // around the known ones must still parse uuid+title.
        let mut c = Vec::new();
        tag(5, 0, &mut c); // unknown varint field
        varint(999, &mut c);
        len_delim(1, b"uuid-xyz", &mut c); // uuid
        tag(6, 1, &mut c); // unknown 64-bit
        c.extend_from_slice(&[0u8; 8]);
        let s = summary("T", None);
        len_delim(2, &s, &mut c); // summary
        tag(7, 5, &mut c); // unknown 32-bit
        c.extend_from_slice(&[0u8; 4]);

        let parsed = parse_conversation(&c).expect("parse");
        assert_eq!(parsed.uuid, "uuid-xyz");
        assert_eq!(parsed.title, "T");
    }
}
