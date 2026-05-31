//! Cursor / VS Code-family SQLite (`state.vscdb`) session decoder.
//!
//! Storage layout (design §3): `…/User/globalStorage/state.vscdb`, a key/value
//! table `cursorDiskKV` whose JSON values include:
//! - `composerData:<id>` — one conversation. JSON with assorted keys across
//!   Cursor versions (`title`, `createdAt`, `conversationMap`,
//!   `fullConversationHeadersOnly`, `richText`, …).
//! - `bubbleId:<composerId>:<bubbleId>` — one message within a conversation.
//!
//! The DB can be **multi-gigabyte and live**, so every access here is:
//! - opened **read-only + `immutable=1`** (never writes, never takes a lock),
//! - queried with SQL `LIMIT` (never `SELECT *` of the whole table),
//! - defensive about schema drift: a missing field degrades (empty title /
//!   skipped row) rather than erroring or panicking.

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::{snippet, SessionReader};
use crate::{BridgeError, Role, SessionRef, SessionStore, Transcript, Turn};

/// Decoder for the Cursor / VS Code-family `state.vscdb` store.
pub struct VscdbReader;

const TITLE_SNIPPET_CHARS: usize = 80;

impl SessionReader for VscdbReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let conn = open_readonly(store)?;
        // Pull composer rows lazily, bounded. We over-fetch slightly only to
        // re-sort by parsed `createdAt`, then truncate to `limit`.
        let mut stmt = conn
            .prepare(
                "SELECT key, value FROM cursorDiskKV \
                 WHERE key LIKE 'composerData:%' LIMIT ?1",
            )
            .map_err(|e| BridgeError::Session(e.to_string()))?;

        // Cap the scan so a giant store never streams unbounded rows.
        let scan_cap = limit.saturating_mul(4).max(limit) as i64;
        let rows = stmt
            .query_map([scan_cap], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| BridgeError::Session(e.to_string()))?;

        let mut refs: Vec<(u64, SessionRef)> = Vec::new();
        for row in rows {
            // A bad row degrades: skip it, keep going.
            let (key, value) = match row {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(id) = key.strip_prefix("composerData:") else {
                continue;
            };
            let parsed: Value = match serde_json::from_str(&value) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Title: the composer's own `title` if set (rare), else the first
            // user message — looked up via the conversation headers, since the
            // text lives on a separate `bubbleId:` row, not on the composer.
            let title = parsed
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|s| snippet(s, TITLE_SNIPPET_CHARS))
                .or_else(|| first_user_title(&conn, id, &parsed));
            let created = parsed.get("createdAt").and_then(Value::as_u64).unwrap_or(0);
            refs.push((
                created,
                SessionRef {
                    id: id.to_string(),
                    title,
                    updated_at: created.to_string(),
                    project: None,
                },
            ));
        }
        // Most-recent first, then bound to limit.
        refs.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
        Ok(refs.into_iter().take(limit).map(|(_, r)| r).collect())
    }

    fn read(&self, store: &SessionStore, id: &str, max_turns: usize) -> anyhow::Result<Transcript> {
        let mut turns = Vec::new();
        if max_turns == 0 {
            return Ok(Transcript { turns });
        }
        let conn = open_readonly(store)?;

        // Load the composer to get its ordered conversation manifest. The
        // message text is NOT here — each header points at a separate
        // `bubbleId:` row.
        let composer = match load_composer(&conn, id) {
            Some(c) => c,
            None => return Ok(Transcript { turns }),
        };
        let headers = conversation_headers(&composer);

        for header in headers {
            if turns.len() >= max_turns {
                break;
            }
            let Some(bubble_id) = header.get("bubbleId").and_then(Value::as_str) else {
                continue;
            };
            let Some(bubble) = fetch_bubble(&conn, id, bubble_id) else {
                continue;
            };
            // The bubble may carry its own role; fall back to the header `type`.
            let mut turn = match bubble_to_turn(&bubble) {
                Some(t) => t,
                None => continue,
            };
            if bubble.get("role").is_none() && bubble.get("type").is_none() {
                turn.role = header_role(header);
            }
            turns.push(turn);
        }
        Ok(Transcript { turns })
    }
}

/// Load and parse a `composerData:<id>` row.
fn load_composer(conn: &Connection, id: &str) -> Option<Value> {
    let key = format!("composerData:{id}");
    let value: String = conn
        .query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1 LIMIT 1",
            [key],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&value).ok()
}

/// The ordered `fullConversationHeadersOnly` list (each `{bubbleId, type}`).
fn conversation_headers(composer: &Value) -> Vec<&Value> {
    composer
        .get("fullConversationHeadersOnly")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Role from a header's numeric `type` (1 = user, 2 = assistant).
fn header_role(header: &Value) -> Role {
    match header.get("type").and_then(Value::as_u64) {
        Some(1) => Role::User,
        Some(2) => Role::Assistant,
        _ => Role::Other,
    }
}

/// Fetch and parse a single `bubbleId:<composer>:<bubble>` row.
fn fetch_bubble(conn: &Connection, composer_id: &str, bubble_id: &str) -> Option<Value> {
    let key = format!("bubbleId:{composer_id}:{bubble_id}");
    let value: String = conn
        .query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1 LIMIT 1",
            [key],
            |row| row.get(0),
        )
        .ok()?;
    serde_json::from_str(&value).ok()
}

/// Best-effort title for a composer: the first user bubble's text, found by
/// walking the conversation headers. Read-only, bounded to the first few
/// headers so listing 220 sessions stays cheap.
fn first_user_title(conn: &Connection, id: &str, composer: &Value) -> Option<String> {
    const MAX_HEADER_SCAN: usize = 12;
    for header in conversation_headers(composer)
        .into_iter()
        .take(MAX_HEADER_SCAN)
    {
        if header.get("type").and_then(Value::as_u64) != Some(1) {
            continue;
        }
        let bubble_id = header.get("bubbleId").and_then(Value::as_str)?;
        if let Some(bubble) = fetch_bubble(conn, id, bubble_id) {
            if let Some(text) = bubble_text(&bubble) {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(snippet(text, TITLE_SNIPPET_CHARS));
                }
            }
        }
    }
    None
}

/// Open the store read-only with `immutable=1` so a live/locked DB is never
/// written or blocked. Never falls back to a writable handle.
fn open_readonly(store: &SessionStore) -> anyhow::Result<Connection> {
    let path = store.path.to_string_lossy();
    // URI form lets us pass `immutable=1`; the flag tells SQLite the file will
    // not change, so it skips locking entirely (safe for our read-only use).
    let uri = format!("file:{path}?immutable=1&mode=ro");
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|_| BridgeError::Unreadable(store.path.clone()).into())
}

/// Map a Cursor bubble JSON value to a normalized [`Turn`], or `None` if it
/// carries no usable text. Role inference is defensive across versions.
fn bubble_to_turn(parsed: &Value) -> Option<Turn> {
    let role = infer_role(parsed);
    let text = bubble_text(parsed)?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Turn { role, text })
}

/// Infer a [`Role`] from a bubble. Cursor has used both a string `role` and a
/// numeric `type` (1 = user, 2 = assistant) across versions; handle both.
fn infer_role(parsed: &Value) -> Role {
    if let Some(s) = parsed.get("role").and_then(Value::as_str) {
        return match s {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => Role::Other,
        };
    }
    match parsed.get("type").and_then(Value::as_u64) {
        Some(1) => Role::User,
        Some(2) => Role::Assistant,
        _ => Role::Other,
    }
}

/// Extract the human-readable text of a bubble across known field names.
fn bubble_text(parsed: &Value) -> Option<String> {
    for field in ["text", "richText", "content"] {
        match parsed.get(field) {
            Some(Value::String(s)) if !s.is_empty() => return Some(s.clone()),
            Some(Value::Array(blocks)) => {
                let mut parts = Vec::new();
                for block in blocks {
                    if let Some(s) = block.get("text").and_then(Value::as_str) {
                        if !s.is_empty() {
                            parts.push(s.to_string());
                        }
                    }
                }
                if !parts.is_empty() {
                    return Some(parts.join("\n"));
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

    /// Build a synthetic `state.vscdb` with a `cursorDiskKV` table.
    fn make_db(path: &std::path::Path) {
        let conn = Connection::open(path).expect("open writable for setup");
        conn.execute(
            "CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("create table");

        // Two composer conversations with differing createdAt for recency.
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params![
                "composerData:conv-older",
                r#"{"title":"Older chat","createdAt":1000}"#
            ],
        )
        .unwrap();
        // conv-new: the real Cursor shape — message text lives on separate
        // `bubbleId:` rows, and the composer's `fullConversationHeadersOnly`
        // is the ORDERED manifest of {bubbleId, type} (1=user, 2=assistant).
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params![
                "composerData:conv-new",
                r#"{"title":"Newer chat","createdAt":2000,"fullConversationHeadersOnly":[{"bubbleId":"b1","type":1},{"bubbleId":"b2","type":2},{"bubbleId":"b3","type":2}]}"#
            ],
        )
        .unwrap();
        // A composer row missing title/createdAt/headers — must degrade.
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params!["composerData:conv-bare", r#"{}"#],
        )
        .unwrap();

        // Bubbles for conv-new, inserted out of order — read() must honor the
        // header order, not key/insertion order.
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params!["bubbleId:conv-new:b2", r#"{"type":2,"text":"Sure, here"}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params!["bubbleId:conv-new:b1", r#"{"type":1,"text":"Question?"}"#],
        )
        .unwrap();
        // A bubble with no text — must be skipped.
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params!["bubbleId:conv-new:b3", r#"{"type":2}"#],
        )
        .unwrap();
    }

    fn store_at(path: std::path::PathBuf) -> SessionStore {
        SessionStore {
            path,
            format: SessionFormat::SqliteVscdb,
        }
    }

    #[test]
    fn list_finds_composers_sorted_by_recency() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_db(&db);

        let reader = VscdbReader;
        let store = store_at(db);
        let refs = reader.list(&store, 10).expect("list");
        // Three composer rows, newest first; bare row degrades to empty title.
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].id, "conv-new");
        assert_eq!(refs[0].title.as_deref(), Some("Newer chat"));
        assert_eq!(refs[1].id, "conv-older");
        let bare = refs.iter().find(|r| r.id == "conv-bare").expect("bare");
        assert!(bare.title.is_none());
    }

    #[test]
    fn read_returns_ordered_turns_and_skips_textless() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_db(&db);

        let reader = VscdbReader;
        let store = store_at(db);
        let t = reader.read(&store, "conv-new", 10).expect("read");
        assert_eq!(t.turns.len(), 2, "textless bubble skipped");
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "Question?");
        assert_eq!(t.turns[1].role, Role::Assistant);
        assert_eq!(t.turns[1].text, "Sure, here");

        // max_turns bound.
        let bounded = reader.read(&store, "conv-new", 1).expect("read bounded");
        assert_eq!(bounded.turns.len(), 1);
    }

    #[test]
    fn open_is_read_only_db_unchanged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_db(&db);
        let before = std::fs::metadata(&db).unwrap().len();

        let store = store_at(db.clone());
        let conn = open_readonly(&store).expect("open ro");
        // A write must fail on a read-only connection.
        let write = conn.execute("INSERT INTO cursorDiskKV VALUES ('x','y')", []);
        assert!(write.is_err(), "write must be rejected on read-only handle");
        drop(conn);

        let after = std::fs::metadata(&db).unwrap().len();
        assert_eq!(before, after, "DB size unchanged after read-only use");
    }

    #[test]
    fn missing_composer_yields_empty_transcript_not_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_db(&db);
        let reader = VscdbReader;
        let store = store_at(db);
        let t = reader.read(&store, "does-not-exist", 10).expect("read");
        assert!(t.turns.is_empty());
    }
}
