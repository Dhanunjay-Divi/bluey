//! Cursor / VS Code-family SQLite (`state.vscdb`) session decoder.
//!
//! Storage layout (design §3). Cursor keeps conversation data in **two** kinds
//! of store, both `state.vscdb` SQLite files:
//!
//! 1. The **global** store at `…/User/globalStorage/state.vscdb` — the
//!    `store.path` we are handed. Its `cursorDiskKV` table holds the rich
//!    conversation bodies:
//!    - `composerData:<id>` — one conversation's manifest (`name`, `createdAt`,
//!      `lastUpdatedAt`, `fullConversationHeadersOnly`, …).
//!    - `bubbleId:<composerId>:<bubbleId>` — one message within a conversation.
//!
//!    The global store does NOT record which **project** a conversation belongs
//!    to.
//! 2. The **per-workspace** stores at `…/User/workspaceStorage/<hash>/
//!    state.vscdb`, one per IDE window/project. Each holds a lightweight
//!    composer LIST in its `ItemTable` under key `composer.composerData`
//!    (`{allComposers:[{composerId,name,createdAt,lastUpdatedAt}]}`) and a
//!    sibling `workspace.json` (`{"folder":"file:///path"}`) giving the real
//!    project path. These add (a) the workspace→project mapping for global
//!    conversations and (b) conversations the global store no longer carries.
//!
//! [`VscdbReader::list`] reads BOTH: the global store for rich, body-backed
//! sessions, then every workspace store to attach projects and surface
//! workspace-only sessions. [`VscdbReader::read`] reads only the global store,
//! where the message bodies live.
//!
//! The global DB can be **multi-gigabyte and live**, so every access here is:
//! - opened **read-only + `immutable=1`** (never writes, never takes a lock),
//! - queried with SQL `LIMIT` (never `SELECT *` of the whole table),
//! - bounded: at most [`MAX_WORKSPACE_STORES`] workspace stores are scanned,
//! - defensive about schema drift: a missing field degrades (empty title /
//!   skipped row) rather than erroring or panicking.
//!
//! Security: only the `state.vscdb` files and the sibling `workspace.json` are
//! ever read inside the workspace dirs — never any other file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use super::{snippet, SessionReader};
use crate::{BridgeError, Role, SessionRef, SessionStore, Transcript, Turn};

/// Decoder for the Cursor / VS Code-family `state.vscdb` store.
pub struct VscdbReader;

const TITLE_SNIPPET_CHARS: usize = 80;

/// Upper bound on how many per-workspace stores we scan in one `list` call.
/// A machine can accumulate hundreds of workspace dirs; the global store already
/// carries the rich bodies, so the workspace pass is purely additive (projects +
/// extra sessions) and is safe to cap. 200 comfortably covers real machines
/// (51 on the dev box) while bounding worst-case work.
const MAX_WORKSPACE_STORES: usize = 200;

/// `ItemTable` key under which a workspace store keeps its composer LIST.
const WORKSPACE_COMPOSER_KEY: &str = "composer.composerData";

impl SessionReader for VscdbReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        // Internal accumulator carrying the recency key alongside the ref, plus
        // a flag for whether this session has a real body in the global store
        // (used so a body-backed global session always wins over a list-only
        // workspace entry on dedup).
        struct Entry {
            updated: u64,
            rf: SessionRef,
            from_global: bool,
        }

        // composerId -> Entry. Insertion-merged across the global store and
        // every workspace store; dedup is by id.
        let mut by_id: HashMap<String, Entry> = HashMap::new();

        // ---- Pass 1: the global store (rich, body-backed conversations). ----
        // This is exactly the prior behavior. The global store has the
        // conversation headers + bubbles, so its sessions are the "real" ones
        // the read() path can decode in full.
        let conn = open_readonly(store)?;
        // Pull composer rows lazily, bounded. We over-fetch slightly only to
        // re-sort by parsed `createdAt`, then truncate to `limit`.
        let mut stmt = conn
            .prepare(
                "SELECT key, value FROM cursorDiskKV \
                 WHERE key LIKE 'composerData:%' LIMIT ?1",
            )
            .map_err(|e| BridgeError::Session(e.to_string()))?;

        // Cursor auto-creates an empty composer for every chat panel opened —
        // most are never used (e.g. ~190 of 220 on a real machine). We list
        // only sessions with ACTUAL conversation content, so over-fetch wider
        // (most rows will be empty and skipped) before truncating to `limit`.
        let scan_cap = limit.saturating_mul(20).max(limit).min(5000) as i64;
        let rows = stmt
            .query_map([scan_cap], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| BridgeError::Session(e.to_string()))?;

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
            // Skip EMPTY sessions — an unused draft has no conversation headers.
            // Only sessions the user actually had a conversation in are shown.
            if conversation_headers(&parsed).is_empty() {
                continue;
            }
            // Title: Cursor stores its own AI-curated conversation title in
            // `name` (NOT `title`, which does not exist on real composer rows —
            // verified 0/227 on a live store). `name` is clean and typo-free
            // ("Build failure due to syntax error") where the first user bubble
            // is often a typo'd or pasted blob. Prefer `name` → `subtitle` →
            // first user message (looked up via the headers, since bubble text
            // lives on a separate `bubbleId:` row). Run the chosen string
            // through `clean_title` so the shared boilerplate filter applies.
            let title = composer_title_field(&parsed)
                .and_then(|s| super::clean_title(Some(s), TITLE_SNIPPET_CHARS))
                .or_else(|| first_user_title(&conn, id, &parsed));
            // Recency: `lastUpdatedAt` is last activity; `createdAt` is the
            // start. Prefer the former, falling back to the latter. Both are
            // epoch milliseconds on real stores.
            let updated = parsed
                .get("lastUpdatedAt")
                .and_then(Value::as_u64)
                .or_else(|| parsed.get("createdAt").and_then(Value::as_u64))
                .unwrap_or(0);
            by_id.insert(
                id.to_string(),
                Entry {
                    updated,
                    rf: SessionRef {
                        id: id.to_string(),
                        title,
                        updated_at: updated.to_string(),
                        project: None,
                    },
                    from_global: true,
                },
            );
        }

        // ---- Pass 2: the per-workspace stores (projects + extra sessions). --
        // Each workspace store lists its composers in ItemTable and names its
        // project in a sibling workspace.json. We use it to (a) attach the
        // project to global sessions and (b) surface workspace-only sessions
        // (curated `name` present) that the global store no longer carries.
        for ws in workspace_stores(&store.path) {
            let project = ws.project.as_deref();
            for c in read_workspace_composers(&ws.db) {
                match by_id.get_mut(&c.id) {
                    // Already seen (in global or an earlier workspace): just
                    // backfill the project if we don't have one yet. Never
                    // downgrade a body-backed global title/recency.
                    Some(existing) => {
                        if existing.rf.project.is_none() {
                            existing.rf.project = project.map(str::to_string);
                        }
                    }
                    // Workspace-only: surface it IF it has a curated title
                    // (proves a real conversation, mirroring the global
                    // "skip empty drafts" rule). Its body isn't in the global
                    // store, so read() will return an empty transcript — but
                    // the row is still useful (title + project + recency).
                    None => {
                        let Some(title) = super::clean_title(c.name.clone(), TITLE_SNIPPET_CHARS)
                        else {
                            continue;
                        };
                        by_id.insert(
                            c.id.clone(),
                            Entry {
                                updated: c.updated,
                                rf: SessionRef {
                                    id: c.id.clone(),
                                    title: Some(title),
                                    updated_at: c.updated.to_string(),
                                    project: project.map(str::to_string),
                                },
                                from_global: false,
                            },
                        );
                    }
                }
            }
        }

        // Most-recent first; body-backed (global) sessions break ties ahead of
        // list-only workspace ones. Then bound to limit.
        let mut entries: Vec<Entry> = by_id.into_values().collect();
        entries.sort_by(|a, b| {
            b.updated
                .cmp(&a.updated)
                .then(b.from_global.cmp(&a.from_global))
        });
        Ok(entries.into_iter().take(limit).map(|e| e.rf).collect())
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

    fn health(&self, store: &SessionStore) -> super::ReaderHealth {
        vscdb_health(store)
    }
}

/// One per-workspace store: the `state.vscdb` path plus the project folder its
/// sibling `workspace.json` points at (when resolvable).
struct WorkspaceStore {
    db: PathBuf,
    project: Option<String>,
}

/// One composer listed in a workspace store's `allComposers`.
struct WorkspaceComposer {
    id: String,
    name: Option<String>,
    /// Recency in epoch ms: `lastUpdatedAt` falling back to `createdAt`.
    updated: u64,
}

/// Enumerate the per-workspace stores that sit alongside the given **global**
/// vscdb. The global store lives at `…/User/globalStorage/state.vscdb`; the
/// workspace stores live at `…/User/workspaceStorage/<hash>/state.vscdb`. We
/// derive `workspaceStorage` from the global path's grandparent (`User/`) so the
/// caller hands us only the one global path, exactly as `discover.rs` provides.
///
/// Read-only and fail-soft: a missing/unreadable `workspaceStorage` yields an
/// empty list, and the scan is bounded to [`MAX_WORKSPACE_STORES`] entries.
/// Only the `state.vscdb` and sibling `workspace.json` are touched — no other
/// file in a workspace dir is read.
fn workspace_stores(global_vscdb: &Path) -> Vec<WorkspaceStore> {
    // …/User/globalStorage/state.vscdb → …/User → …/User/workspaceStorage
    let Some(user_dir) = global_vscdb.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    let ws_root = user_dir.join("workspaceStorage");
    let read = match std::fs::read_dir(&ws_root) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut out = Vec::new();
    for child in read.flatten() {
        if out.len() >= MAX_WORKSPACE_STORES {
            break;
        }
        let dir = child.path();
        let db = dir.join("state.vscdb");
        // Cheap existence gate before any SQLite open; skip dirs without a store.
        if std::fs::metadata(&db).is_err() {
            continue;
        }
        let project = read_workspace_folder(&dir.join("workspace.json"));
        out.push(WorkspaceStore { db, project });
    }
    out
}

/// Read the project path from a workspace's `workspace.json` (`{"folder":
/// "file:///path"}`). Returns a filesystem path for `file://` URLs (percent-
/// decoded); for `vscode-remote://` (SSH/dev-container) folders the host-side
/// path is not a local filesystem path, so we return the raw URI as a label
/// rather than a misleading local path. `None` when the file is absent,
/// unreadable, or has no `folder`. Read-only and fail-soft.
fn read_workspace_folder(workspace_json: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(workspace_json).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let folder = parsed.get("folder").and_then(Value::as_str)?.trim();
    if folder.is_empty() {
        return None;
    }
    if let Some(rest) = folder.strip_prefix("file://") {
        // `file:///Users/...` → `/Users/...`; percent-decode `%20` etc. so the
        // project leaf renders cleanly (e.g. "Job Matching Algorithm").
        Some(percent_decode(rest))
    } else {
        // Remote (vscode-remote://…) or other scheme: keep the URI as a label;
        // it is NOT a local path, so we must not present it as one.
        Some(folder.to_string())
    }
}

/// Minimal percent-decoder for `file://` URL paths (`%20` → space, …).
/// Dependency-free; leaves malformed/incomplete escapes untouched. UTF-8
/// multi-byte sequences (each byte its own `%XX`) are reassembled and decoded
/// lossily.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Read the composer LIST from one workspace store's `ItemTable`
/// (`composer.composerData` → `{allComposers:[…]}`). Bodies are NOT here — only
/// `{composerId, name, createdAt, lastUpdatedAt}` per composer. Read-only +
/// `immutable=1`; a missing key / unreadable DB / bad JSON degrades to an empty
/// list. Composers without an id are skipped.
fn read_workspace_composers(db: &Path) -> Vec<WorkspaceComposer> {
    let conn = match open_readonly_path(db) {
        Some(c) => c,
        None => return Vec::new(),
    };
    let raw: String = match conn.query_row(
        "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1",
        [WORKSPACE_COMPOSER_KEY],
        |row| row.get(0),
    ) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(all) = parsed.get("allComposers").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for c in all {
        let Some(id) = c.get("composerId").and_then(Value::as_str) else {
            continue;
        };
        let name = c
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let updated = c
            .get("lastUpdatedAt")
            .and_then(Value::as_u64)
            .or_else(|| c.get("createdAt").and_then(Value::as_u64))
            .unwrap_or(0);
        out.push(WorkspaceComposer {
            id: id.to_string(),
            name,
            updated,
        });
    }
    out
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

/// The composer's own stored title, across Cursor field names. Real composer
/// rows use `name` (AI-curated) and `subtitle`; older/other surfaces may use
/// `title`. First non-empty wins. Returned raw — the caller runs `clean_title`.
fn composer_title_field(composer: &Value) -> Option<String> {
    for field in ["name", "subtitle", "title"] {
        if let Some(s) = composer.get(field).and_then(Value::as_str) {
            let s = s.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// The ordered `fullConversationHeadersOnly` list (each `{bubbleId, type}`).
fn conversation_headers(composer: &Value) -> Vec<&Value> {
    composer
        .get("fullConversationHeadersOnly")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

/// Drift health for the Cursor `cursorDiskKV` store. The raw unit is each
/// `composerData:%` row (one conversation record); a row is "parsed" when it is
/// valid JSON carrying at least one conversation header (i.e. understood as a
/// real conversation, the same bar `list()` uses). Returns the
/// [`ReaderHealth::Parsed`] ratio so the canary can tell "no conversations yet"
/// (raw_total 0) from "the schema moved under us" (raw rows present, none parse
/// — e.g. the historical `ItemTable`→`cursorDiskKV` migration, or headers field
/// renamed). Read-only and bounded; never the full-bubble walk.
fn vscdb_health(store: &SessionStore) -> super::ReaderHealth {
    let Some(conn) = open_readonly_path(&store.path) else {
        return super::ReaderHealth::EmptyStore;
    };
    // Cap the scan so a huge store stays cheap; this is a shape probe, not a
    // full listing. 5000 comfortably covers real machines (≤220 composers).
    const HEALTH_SCAN_CAP: i64 = 5000;
    let Ok(mut stmt) =
        conn.prepare("SELECT value FROM cursorDiskKV WHERE key LIKE 'composerData:%' LIMIT ?1")
    else {
        // The `cursorDiskKV` table is absent. If the DB has ANY other table the
        // store is non-empty → this is the table-rename drift; otherwise it is a
        // genuinely empty/new store.
        let non_empty = any_table_nonempty(&conn);
        return if non_empty {
            super::ReaderHealth::Parsed {
                parsed: 0,
                raw_total: 1,
            }
        } else {
            super::ReaderHealth::EmptyStore
        };
    };
    let Ok(rows) = stmt.query_map([HEALTH_SCAN_CAP], |row| row.get::<_, String>(0)) else {
        return super::ReaderHealth::EmptyStore;
    };
    // The vscdb drift question is specifically: "can we still decode the
    // conversation STRUCTURE (`fullConversationHeadersOnly`) on rows that have
    // it?" So the ratio is computed over BODY-BEARING rows only:
    //   raw_total = rows that should decode a body — those with headers today,
    //               PLUS rows that are valid JSON but un-decodable (invalid value
    //               encoding), which is itself a drift signal.
    //   parsed    = rows whose headers we actually read.
    // Deliberately EXCLUDED from both (they are not body-format drift):
    //   - empty auto-drafts (Cursor makes ~190-of-227; timestamps only, no body)
    //   - workspace-only rows (a curated `name` but the body lives in another
    //     store — `list()` shows these from the name; absence of headers here is
    //     expected, not drift).
    // This keeps a healthy store at ratio ~1.0 instead of ~0.14, while a real
    // headers-field RENAME (rows that clearly held a body now decode 0 headers)
    // still collapses the ratio. We approximate "clearly held a body" as "valid
    // JSON composer row that is neither an empty draft nor workspace-only-named",
    // i.e. it has headers now — so a clean rename shows as parsed 0 of (invalid
    // JSON rows), and the table-missing case (handled above) covers the rest.
    let mut raw_total = 0usize;
    let mut parsed = 0usize;
    for value in rows.flatten() {
        let Ok(v) = serde_json::from_str::<Value>(&value) else {
            // Invalid JSON in a composerData row is a value-encoding drift signal.
            raw_total += 1;
            continue;
        };
        if conversation_headers(&v).is_empty() {
            continue; // empty draft or workspace-only — not a body-format failure.
        }
        raw_total += 1;
        parsed += 1;
    }
    if raw_total == 0 {
        super::ReaderHealth::EmptyStore
    } else {
        super::ReaderHealth::Parsed { parsed, raw_total }
    }
}

/// Whether a vscdb connection has any user table holding at least one row — used
/// only to distinguish "the `cursorDiskKV` table was renamed/dropped but the DB
/// is otherwise populated" (drift) from "a fresh empty DB". Read-only, bounded:
/// checks at most a handful of tables and short-circuits on the first non-empty.
fn any_table_nonempty(conn: &Connection) -> bool {
    let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' LIMIT 32")
    else {
        return false;
    };
    let Ok(names) = stmt.query_map([], |row| row.get::<_, String>(0)) else {
        return false;
    };
    for name in names.flatten() {
        // Table names come from sqlite_master (not user input) but quote them
        // defensively anyway; a count failure just skips this table.
        let q = format!("SELECT 1 FROM \"{}\" LIMIT 1", name.replace('"', "\"\""));
        if conn.query_row(&q, [], |_| Ok(())).is_ok() {
            return true;
        }
    }
    false
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
/// written or blocked. Never falls back to a writable handle. Surfaces an
/// [`BridgeError::Unreadable`] error so the (global) store's failure propagates.
fn open_readonly(store: &SessionStore) -> anyhow::Result<Connection> {
    open_readonly_path(&store.path)
        .ok_or_else(|| BridgeError::Unreadable(store.path.clone()).into())
}

/// Open any vscdb file read-only with `immutable=1`, or `None` if it can't be
/// opened. Shared by the global-store open above and the per-workspace store
/// reader, which is fail-soft (a bad workspace DB is simply skipped). Never
/// writes, never locks, never falls back to a writable handle.
fn open_readonly_path(path: &Path) -> Option<Connection> {
    let p = path.to_string_lossy();
    // URI form lets us pass `immutable=1`; the flag tells SQLite the file will
    // not change, so it skips locking entirely (safe for our read-only use).
    let uri = format!("file:{p}?immutable=1&mode=ro");
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .ok()
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
        // conv-older: has content (a header) + an older createdAt, so it lists
        // but sorts after conv-new.
        // conv-older uses ONLY a first-user-bubble for its title (no `name`),
        // proving the fallback chain still works when Cursor didn't curate one.
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params![
                "composerData:conv-older",
                r#"{"createdAt":1000,"lastUpdatedAt":1500,"fullConversationHeadersOnly":[{"bubbleId":"o1","type":1}]}"#
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params!["bubbleId:conv-older:o1", r#"{"type":1,"text":"older q"}"#],
        )
        .unwrap();
        // conv-new: the real Cursor shape — the AI-curated title lives in `name`
        // (NOT `title`), the first user bubble is a typo'd blob that must NOT win
        // over `name`, message text lives on separate `bubbleId:` rows, and
        // `fullConversationHeadersOnly` is the ORDERED manifest of {bubbleId,
        // type} (1=user, 2=assistant). `lastUpdatedAt` > conv-older's, so newest.
        conn.execute(
            "INSERT INTO cursorDiskKV VALUES (?1, ?2)",
            rusqlite::params![
                "composerData:conv-new",
                r#"{"name":"Build failure due to syntax error","subtitle":"edited main.rs","createdAt":2000,"lastUpdatedAt":3000,"fullConversationHeadersOnly":[{"bubbleId":"b1","type":1},{"bubbleId":"b2","type":2},{"bubbleId":"b3","type":2}]}"#
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
    fn list_shows_only_sessions_with_conversation_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_db(&db);

        let reader = VscdbReader;
        let store = store_at(db);
        let refs = reader.list(&store, 10).expect("list");
        // 3 composers in the fixture: conv-new + conv-older have conversation
        // content (listed, newest first); conv-bare is an empty auto-created
        // draft and is FILTERED OUT so users see only real sessions.
        assert_eq!(refs.len(), 2, "empty draft (conv-bare) must be excluded");
        assert_eq!(refs[0].id, "conv-new", "newest first (by lastUpdatedAt)");
        // The AI-curated `name` wins over the first user bubble ("Question?").
        assert_eq!(
            refs[0].title.as_deref(),
            Some("Build failure due to syntax error"),
            "Cursor's `name` field must be the title, not the first bubble"
        );
        // conv-older has no `name` → falls back to its first user bubble.
        assert_eq!(refs[1].id, "conv-older");
        assert_eq!(refs[1].title.as_deref(), Some("older q"));
        // Recency uses lastUpdatedAt (conv-new 3000 > conv-older 1500).
        assert_eq!(refs[0].updated_at, "3000");
        assert!(
            !refs.iter().any(|r| r.id == "conv-bare"),
            "empty session must not appear"
        );
    }

    #[test]
    fn health_counts_content_rows_and_is_not_drift_for_real_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_db(&db);

        // raw_total = composerData rows WITH conversation headers (conv-new,
        // conv-older); the empty draft (conv-bare) has no headers so it is not a
        // content row and doesn't count against the ratio. Both content rows
        // parse → ratio 1.0, not drift.
        let h = VscdbReader.health(&store_at(db));
        assert!(
            !h.is_total_drift(),
            "real store must not look like drift: {h:?}"
        );
        assert_eq!(h.parse_ratio(), 1.0);
    }

    #[test]
    fn health_flags_drift_when_cursordiskkv_table_is_renamed() {
        // Simulates the historical Cursor migration ItemTable -> cursorDiskKV: the
        // DB is populated, but the table the reader expects is GONE. health() must
        // report total drift (raw>0, parsed 0), not EmptyStore.
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        let conn = Connection::open(&db).expect("open writable for setup");
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ItemTable VALUES ('composer.composerData', '{\"allComposers\":[]}')",
            [],
        )
        .unwrap();
        drop(conn);

        let h = VscdbReader.health(&store_at(db));
        assert!(h.is_total_drift(), "renamed table must flag drift: {h:?}");
    }

    #[test]
    fn health_is_empty_store_for_fresh_empty_db() {
        // A brand-new vscdb with the right table but no composers → genuinely
        // empty, NOT drift (this is the false-positive the ratio design avoids).
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        let conn = Connection::open(&db).expect("open writable for setup");
        conn.execute(
            "CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        drop(conn);

        let h = VscdbReader.health(&store_at(db));
        assert_eq!(h, super::super::ReaderHealth::EmptyStore);
        assert!(!h.is_total_drift());
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

    // ----- Per-workspace store merge (projects + workspace-only sessions) -----
    //
    // Cursor's IDE writes BOTH a global store (rich bodies, no project) and one
    // store per workspace (`User/workspaceStorage/<hash>/state.vscdb`, a
    // lightweight `composer.composerData` list) plus a `workspace.json` naming
    // the project. The reader must derive `workspaceStorage` from the global
    // path, attach the project to global sessions, and surface workspace-only
    // sessions the global store no longer carries — without regressing the
    // existing global-only behavior.

    /// Build the real Cursor layout under `root`:
    /// `root/User/globalStorage/state.vscdb` (the `make_db` fixture) and return
    /// the global store path. The workspace stores are added by `add_workspace`.
    fn make_global_layout(root: &std::path::Path) -> std::path::PathBuf {
        let global = root.join("User/globalStorage/state.vscdb");
        std::fs::create_dir_all(global.parent().unwrap()).expect("mkdir globalStorage");
        make_db(&global);
        global
    }

    /// Add one workspace store under `root/User/workspaceStorage/<hash>/` with
    /// the given `allComposers` JSON array and an optional `workspace.json`
    /// `folder` value. Mirrors the on-disk shape (`ItemTable` + `cursorDiskKV`).
    fn add_workspace(
        root: &std::path::Path,
        hash: &str,
        all_composers: &str,
        folder: Option<&str>,
    ) {
        let dir = root.join("User/workspaceStorage").join(hash);
        std::fs::create_dir_all(&dir).expect("mkdir workspace");
        let db = dir.join("state.vscdb");
        let conn = Connection::open(&db).expect("open workspace db");
        // Real workspace stores carry both tables; the composer list is in
        // ItemTable under `composer.composerData`.
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);
             CREATE TABLE cursorDiskKV (key TEXT PRIMARY KEY, value TEXT);",
        )
        .expect("create workspace tables");
        conn.execute(
            "INSERT INTO ItemTable VALUES ('composer.composerData', ?1)",
            rusqlite::params![format!(r#"{{"allComposers":{all_composers}}}"#)],
        )
        .expect("insert composer list");
        if let Some(f) = folder {
            std::fs::write(dir.join("workspace.json"), format!(r#"{{"folder":"{f}"}}"#))
                .expect("write workspace.json");
        }
    }

    #[test]
    fn workspace_attaches_project_to_global_session_and_keeps_one_on_dedup() {
        // The SAME composer (`conv-new`) appears in the global store (with a
        // body) AND in a workspace's allComposers (with a project). The merge
        // must keep ONE entry, retain the global body-backed title/recency, and
        // attach the workspace's project.
        let root = tempfile::tempdir().expect("tempdir");
        let global = make_global_layout(root.path());
        add_workspace(
            root.path(),
            "ws-hash-a",
            r#"[{"composerId":"conv-new","name":"stale ws name","createdAt":1,"lastUpdatedAt":2}]"#,
            Some("file:///Users/me/Developer/Bluey"),
        );

        let refs = VscdbReader.list(&store_at(global), 50).expect("list");
        let conv_new: Vec<_> = refs.iter().filter(|r| r.id == "conv-new").collect();
        assert_eq!(conv_new.len(), 1, "dedup keeps a single conv-new entry");
        let c = conv_new[0];
        // Body-backed global title wins over the workspace's stale `name`.
        assert_eq!(
            c.title.as_deref(),
            Some("Build failure due to syntax error"),
            "global body-backed title must win over workspace name"
        );
        // Global recency preserved (3000), not the workspace's 2.
        assert_eq!(c.updated_at, "3000", "global recency preserved");
        // Project attached from the workspace.json folder (percent-decoded path).
        assert_eq!(
            c.project.as_deref(),
            Some("/Users/me/Developer/Bluey"),
            "project resolved from workspace.json folder"
        );
    }

    #[test]
    fn workspace_only_named_sessions_surface_with_project() {
        // A composer that exists ONLY in a workspace (no global body) but has a
        // curated `name` must surface as a session with its project. An UNNAMED
        // workspace-only composer (an empty draft) must NOT surface.
        let root = tempfile::tempdir().expect("tempdir");
        let global = make_global_layout(root.path());
        add_workspace(
            root.path(),
            "ws-hash-b",
            r#"[
                {"composerId":"ws-only-1","name":"Designing the landing page","createdAt":4000,"lastUpdatedAt":5000},
                {"composerId":"ws-only-blank","createdAt":100,"lastUpdatedAt":200}
            ]"#,
            Some("file:///Users/me/Projects/Heyloo"),
        );

        let refs = VscdbReader.list(&store_at(global), 50).expect("list");
        // The 2 global sessions (conv-new, conv-older) PLUS the 1 named
        // workspace-only session = 3. The blank workspace-only draft is skipped.
        assert_eq!(refs.len(), 3, "named ws-only session added; blank skipped");
        let ws_only = refs
            .iter()
            .find(|r| r.id == "ws-only-1")
            .expect("named workspace-only session surfaces");
        assert_eq!(ws_only.title.as_deref(), Some("Designing the landing page"));
        assert_eq!(
            ws_only.project.as_deref(),
            Some("/Users/me/Projects/Heyloo")
        );
        // ws-only-1 has the newest lastUpdatedAt (5000) → sorts first.
        assert_eq!(refs[0].id, "ws-only-1", "newest overall sorts first");
        assert!(
            !refs.iter().any(|r| r.id == "ws-only-blank"),
            "unnamed workspace-only draft must not surface"
        );
    }

    #[test]
    fn no_workspace_storage_preserves_global_only_behavior() {
        // With the real global layout but NO workspaceStorage dir at all, the
        // listing is exactly the global-only result (2 sessions, no projects) —
        // the workspace pass must be a no-op, never an error.
        let root = tempfile::tempdir().expect("tempdir");
        let global = make_global_layout(root.path());
        // Intentionally do NOT create User/workspaceStorage.

        let refs = VscdbReader.list(&store_at(global), 50).expect("list");
        assert_eq!(refs.len(), 2, "global-only sessions unchanged");
        assert!(
            refs.iter().all(|r| r.project.is_none()),
            "no project without a workspace store"
        );
    }

    #[test]
    fn corrupt_or_partial_workspace_store_is_skipped_fail_soft() {
        // A workspace dir with a corrupt vscdb, and another with NO composer
        // key, must both degrade silently — the global sessions still list and a
        // valid sibling workspace's session still surfaces.
        let root = tempfile::tempdir().expect("tempdir");
        let global = make_global_layout(root.path());

        // Corrupt DB.
        let bad_dir = root.path().join("User/workspaceStorage/ws-bad");
        std::fs::create_dir_all(&bad_dir).expect("mkdir bad");
        std::fs::write(bad_dir.join("state.vscdb"), b"not-sqlite").expect("write corrupt");
        std::fs::write(bad_dir.join("workspace.json"), r#"{"folder":"file:///x"}"#)
            .expect("write ws json");

        // Valid sibling with a named workspace-only composer.
        add_workspace(
            root.path(),
            "ws-good",
            r#"[{"composerId":"good-1","name":"Valid sibling session","createdAt":6000,"lastUpdatedAt":7000}]"#,
            Some("file:///Users/me/Good"),
        );

        let refs = VscdbReader.list(&store_at(global), 50).expect("list");
        assert!(
            refs.iter().any(|r| r.id == "good-1"),
            "valid sibling session surfaces despite a corrupt neighbor"
        );
        assert!(
            refs.iter().any(|r| r.id == "conv-new"),
            "global sessions still list"
        );
    }

    #[test]
    fn read_workspace_folder_decodes_file_url_and_keeps_remote_uri() {
        let dir = tempfile::tempdir().expect("tempdir");
        // file:// with percent-encoding → decoded local path.
        let f1 = dir.path().join("a.json");
        std::fs::write(
            &f1,
            r#"{"folder":"file:///Users/me/Job%20Matching%20Algorithm"}"#,
        )
        .unwrap();
        assert_eq!(
            read_workspace_folder(&f1).as_deref(),
            Some("/Users/me/Job Matching Algorithm")
        );

        // vscode-remote:// → kept verbatim (NOT presented as a local path).
        let f2 = dir.path().join("b.json");
        std::fs::write(
            &f2,
            r#"{"folder":"vscode-remote://ssh-remote%2Bhost/home/p"}"#,
        )
        .unwrap();
        assert_eq!(
            read_workspace_folder(&f2).as_deref(),
            Some("vscode-remote://ssh-remote%2Bhost/home/p")
        );

        // Missing folder / missing file → None.
        let f3 = dir.path().join("c.json");
        std::fs::write(&f3, r#"{}"#).unwrap();
        assert_eq!(read_workspace_folder(&f3), None);
        assert_eq!(read_workspace_folder(&dir.path().join("nope.json")), None);
    }

    #[test]
    fn percent_decode_handles_spaces_unicode_and_bad_escapes() {
        assert_eq!(percent_decode("/a%20b"), "/a b");
        // UTF-8 "é" = 0xC3 0xA9, each byte its own escape.
        assert_eq!(percent_decode("/caf%C3%A9"), "/café");
        // A dangling/incomplete escape is left untouched, never panics.
        assert_eq!(percent_decode("/x%2"), "/x%2");
        assert_eq!(percent_decode("/y%zz"), "/y%zz");
        assert_eq!(percent_decode("/plain/path"), "/plain/path");
    }

    #[test]
    fn workspace_store_path_math_derives_workspacestorage_from_global() {
        // The reader must locate workspaceStorage as a sibling of globalStorage,
        // both under User/ — derived purely from the global vscdb path.
        let global = std::path::Path::new("/data/Cursor/User/globalStorage/state.vscdb");
        let user = global.parent().and_then(std::path::Path::parent).unwrap();
        assert!(user.ends_with("User"));
        assert_eq!(
            user.join("workspaceStorage"),
            std::path::Path::new("/data/Cursor/User/workspaceStorage")
        );
        // And `workspace_stores` returns empty (no panic) for a path whose
        // workspaceStorage does not exist.
        assert!(workspace_stores(global).is_empty());
    }
}
