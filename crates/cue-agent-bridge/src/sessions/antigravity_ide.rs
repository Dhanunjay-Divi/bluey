//! Antigravity **IDE** session reader.
//!
//! The Antigravity IDE is a SEPARATE desktop app from the Antigravity (2.0) app
//! that [`antigravity`](super::antigravity) reads. Its conversations are
//! invisible to that reader because the IDE store has **no**
//! `agyhub_summaries_proto.pb` index — so the 2.0 reader's `list()` finds
//! nothing there. The two stores split as follows:
//!
//! - **Bodies** live under `~/.gemini/antigravity-ide/`, in the SAME readable
//!   formats the 2.0 store uses: `conversations/<uuid>.db` (plaintext SQLite
//!   `steps`) and `brain/<uuid>/.system_generated/logs/transcript.jsonl`
//!   (JSONL). So [`read`](AntigravityIdeReader::read) just delegates to the
//!   shared [`super::antigravity::read_body_from_root`] with the IDE root — no
//!   body logic is duplicated.
//! - **The session INDEX** lives in the IDE's VS Code-style state store
//!   (`~/Library/Application Support/Antigravity IDE/User/globalStorage/
//!   state.vscdb`, opened read-only) in the `ItemTable` row keyed
//!   `antigravityUnifiedStateSync.trajectorySummaries`. Its value is a
//!   base64-encoded protobuf of `(uuid, title, project)` rows.
//!
//! ### `trajectorySummaries` wire format (byte-verified on a real machine)
//!
//! The ItemTable value is base64; decoded it is a protobuf of:
//! - field 1 (repeated): one `TrajectorySummary` per conversation
//!   - field 1: `uuid` (string)
//!   - field 2: a wrapper message whose **field 1** is a **base64-encoded**
//!     inner `Summary` protobuf (byte-verified: entry field 2 is NOT itself
//!     base64 — it is a protobuf whose field 1 carries the base64 text). The
//!     decoded `Summary` holds:
//!     - field 1: `title` (string, e.g. "Bluey Landing Page Handoff")
//!     - field 9: `Workspace`, whose field 1 is a `file://` project URI.
//!
//! The reader is tolerant by design (mirroring every other reader): it walks
//! fields by wire type, takes only the string fields at the known tags, and
//! skips everything else generically. A row whose inner blob fails to
//! base64-decode or parse simply contributes a title-less entry (the caller's
//! project fallback then labels it), never a panic.
//!
//! Security invariants match every reader: read-only (SQLite opened
//! `SQLITE_OPEN_READ_ONLY` + `immutable=1`), bounded, fail-soft (no
//! `unwrap`/`expect` on external data, no panic on malformed input), and
//! **secret-free** — ONLY the single `trajectorySummaries` key is read; the
//! sibling `antigravityUnifiedStateSync.oauthToken` key (and every other) is
//! never touched.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use rusqlite::{Connection, OpenFlags};

use super::antigravity::{body_path_for_root, lossy, read_body_from_root, strip_file_scheme, Buf};
use super::{mtime_epoch_string, SessionReader};
use crate::{SessionRef, SessionStore, Transcript};

/// Reader for [`SessionFormat::AntigravityIdeIndex`](crate::SessionFormat::AntigravityIdeIndex).
///
/// `SessionStore::path` points at the IDE's `state.vscdb` **index file** (in
/// App-Support), NOT the data-dir root. Bodies are resolved from a SEPARATE
/// tree, `$HOME/.gemini/antigravity-ide`, which the reader derives from `$HOME`
/// (the index and the bodies are in different locations on disk).
pub struct AntigravityIdeReader;

/// The `ItemTable` key whose value is the base64-protobuf session index.
const TRAJECTORY_SUMMARIES_KEY: &str = "antigravityUnifiedStateSync.trajectorySummaries";
/// Cap chars kept for a derived title (matches the 2.0 reader).
const TITLE_MAX_CHARS: usize = 90;

impl SessionReader for AntigravityIdeReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let summaries = parse_trajectory_summaries(&store.path);
        let bodies = ide_bodies_dir();

        let mut refs: Vec<SessionRef> = summaries
            .into_iter()
            .map(|s| {
                // Recency: mtime of the conversation body if present, else the
                // index file's mtime (the index carries unlabeled protobuf
                // timestamps we don't rely on — same posture as the 2.0 reader).
                let updated_at = bodies
                    .as_deref()
                    .and_then(|root| body_path_for_root(root, &s.uuid))
                    .map(|p| mtime_epoch_string(&p))
                    .unwrap_or_else(|| mtime_epoch_string(&store.path));
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

    fn read(
        &self,
        _store: &SessionStore,
        id: &str,
        max_turns: usize,
    ) -> anyhow::Result<Transcript> {
        // Bodies live under `~/.gemini/antigravity-ide` (NOT next to the
        // App-Support index). Delegate to the SHARED 2.0 body reader with the
        // IDE root — same `.db`/brain formats. No readable bodies dir → empty.
        match ide_bodies_dir() {
            Some(root) => Ok(read_body_from_root(&root, id, max_turns)),
            None => Ok(Transcript { turns: Vec::new() }),
        }
    }

    /// Drift health for the IDE `trajectorySummaries` index. The raw unit is each
    /// `TrajectorySummary` record in the decoded protobuf; `parsed` is how many
    /// decoded into a `Summary` with a uuid. If the `state.vscdb`/`ItemTable` key
    /// is absent the store is empty; if the key is present but decodes to 0
    /// records on a non-empty blob, that is the format-drift signal (the base64/
    /// wire shape changed) — reported as `parsed 0 of raw>0`.
    fn health(&self, store: &SessionStore) -> super::ReaderHealth {
        let Some(b64) = read_trajectory_value(&store.path) else {
            return super::ReaderHealth::EmptyStore; // no vscdb / no key
        };
        let trimmed = b64.trim();
        if trimmed.is_empty() {
            return super::ReaderHealth::EmptyStore;
        }
        let parsed = parse_trajectory_summaries_bytes(trimmed.as_bytes()).len();
        let raw = count_trajectory_records(trimmed.as_bytes());
        // The blob is present and non-empty; if neither decoded any records, that
        // is drift — report one unparsed record rather than EmptyStore.
        let raw_total = raw.max(if parsed == 0 { 1 } else { parsed });
        super::ReaderHealth::Parsed { parsed, raw_total }
    }
}

/// Read the raw `trajectorySummaries` value (base64 text) from the IDE
/// `state.vscdb`, read-only. `None` when the DB can't be opened or the key is
/// absent. Factored out so both `list`/`health` share the exact same lookup.
fn read_trajectory_value(vscdb: &Path) -> Option<String> {
    let conn = open_readonly(vscdb)?;
    conn.query_row(
        "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1",
        [TRAJECTORY_SUMMARIES_KEY],
        |row| {
            row.get::<_, Option<String>>(0).or_else(|_| {
                row.get::<_, Option<Vec<u8>>>(0)
                    .map(|o| o.map(|b| String::from_utf8_lossy(&b).into_owned()))
            })
        },
    )
    .ok()
    .flatten()
}

/// Count raw `TrajectorySummary` records in the decoded blob WITHOUT fully
/// parsing each — the health denominator (mirrors the 2.0 reader's
/// `count_index_records`). Fail-soft: a base64/proto problem yields 0.
fn count_trajectory_records(b64: &[u8]) -> usize {
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return 0;
    };
    let mut buf = Buf::new(&decoded);
    let mut n = 0usize;
    while let Some((field, wire)) = buf.tag() {
        if field == 1 && wire == 2 {
            if buf.len_delim().is_some() {
                n += 1;
            } else {
                break;
            }
        } else if !buf.skip(wire) {
            break;
        }
    }
    n
}

/// The Antigravity IDE bodies directory, `$HOME/.gemini/antigravity-ide`. This
/// is a DIFFERENT location from the App-Support `state.vscdb` index, so it is
/// derived from `$HOME` rather than from the index path. `None` if `$HOME` is
/// unset (the listing then degrades to index-only recency / empty reads).
fn ide_bodies_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(Path::new(&home).join(".gemini").join("antigravity-ide"))
}

/// One conversation row extracted from the IDE `trajectorySummaries` index.
struct Summary {
    uuid: String,
    title: String,
    project: Option<String>,
}

/// Open the IDE `state.vscdb` read-only and parse the `trajectorySummaries`
/// value into conversation summaries. Fail-soft: a missing/unreadable DB, a
/// missing key, or a garbled value all yield an empty list (the caller then
/// lists nothing for the IDE, never errors). Reads ONLY the one key — never the
/// sibling `oauthToken` (or any other) row.
fn parse_trajectory_summaries(vscdb: &Path) -> Vec<Summary> {
    // SQLite returns the value as TEXT (base64) on real stores; `read_trajectory_
    // value` also accepts a BLOB form defensively. `None` → no DB / no key.
    let Some(b64) = read_trajectory_value(vscdb) else {
        return Vec::new();
    };
    parse_trajectory_summaries_bytes(b64.trim().as_bytes())
}

/// Parse the (base64) `trajectorySummaries` value bytes into summaries. Split
/// out from the SQLite read so it is unit-testable against a captured byte
/// sample. Fail-soft at every step.
fn parse_trajectory_summaries_bytes(b64: &[u8]) -> Vec<Summary> {
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return Vec::new();
    };
    parse_index_proto(&decoded)
}

/// Parse the decoded outer protobuf: `repeated TrajectorySummary` at field 1.
fn parse_index_proto(bytes: &[u8]) -> Vec<Summary> {
    let mut out = Vec::new();
    let mut buf = Buf::new(bytes);
    while let Some((field, wire)) = buf.tag() {
        if field == 1 && wire == 2 {
            if let Some(entry) = buf.len_delim() {
                if let Some(s) = parse_entry(entry) {
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

/// Parse one `TrajectorySummary`: field 1 = uuid, field 2 = a wrapper message
/// whose field 1 holds the base64-encoded inner `Summary`. A row needs the
/// uuid; title/project may be absent (the caller falls back). Unknown fields
/// are skipped generically.
fn parse_entry(bytes: &[u8]) -> Option<Summary> {
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
                // Field 2 is a wrapper protobuf; its field 1 carries the
                // base64-encoded inner Summary. Parse the wrapper, then decode.
                if let Some(wrapper) = buf.len_delim() {
                    if let Some((t, p)) = parse_summary_wrapper(wrapper) {
                        title = t;
                        project = p;
                    }
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
    Some(Summary {
        uuid,
        title: title.unwrap_or_default(),
        project,
    })
}

/// Parse the entry's field-2 wrapper message: take its field 1 (the base64
/// text), then decode + parse the inner `Summary`. The base64 lives at field 1
/// (wire 2). Fail-soft: a missing field 1 / bad base64 yields `None`.
fn parse_summary_wrapper(wrapper: &[u8]) -> Option<(Option<String>, Option<String>)> {
    let mut buf = Buf::new(wrapper);
    while let Some((field, wire)) = buf.tag() {
        if field == 1 && wire == 2 {
            let b64 = buf.len_delim()?;
            return parse_inner_summary(b64);
        } else if !buf.skip(wire) {
            break;
        }
    }
    None
}

/// Decode + parse the inner `Summary` blob: base64 → protobuf with field 1 =
/// title, field 9 = `Workspace` (field 1 = `file://` uri). Returns
/// `(title, project)`. A blob that won't base64-decode or parse yields `None`
/// (the row then has no title/project, handled upstream).
fn parse_inner_summary(b64: &[u8]) -> Option<(Option<String>, Option<String>)> {
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let mut buf = Buf::new(&decoded);
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
    Some((title, project))
}

/// Parse a `Workspace` message and return its `file://` uri (field 1), stripped
/// of the scheme and percent-decoded so it reads as a plain path. `None` if
/// absent/empty.
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

/// Open the IDE `state.vscdb` read-only with `immutable=1` (never writes, never
/// locks a live DB), or `None` if it can't be opened. Mirrors the vscdb reader's
/// open pattern.
fn open_readonly(path: &Path) -> Option<Connection> {
    let p = path.to_string_lossy();
    let uri = format!("file:{p}?immutable=1&mode=ro");
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role, SessionFormat};

    // ---- protobuf/base64 encoding helpers, to build a synthetic index ----
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
    /// The inner `Summary`: field 1 = title, field 9 = workspace. Exercises an
    /// unknown intermediate field (field 5 varint) so the skip path is covered.
    fn inner_summary(title: &str, ws: Option<&str>) -> Vec<u8> {
        let mut s = Vec::new();
        len_delim(1, title.as_bytes(), &mut s); // field 1 = title
        tag(5, 0, &mut s); // unknown varint field → skip path
        varint(42, &mut s);
        if let Some(uri) = ws {
            let w = workspace(uri);
            len_delim(9, &w, &mut s); // field 9 = workspace
        }
        s
    }
    /// One `TrajectorySummary`: field 1 = uuid, field 2 = a WRAPPER message
    /// whose field 1 = BASE64(inner summary) — mirroring the real on-disk
    /// structure (entry.field2 is a protobuf, not the base64 directly).
    fn entry(uuid: &str, title: &str, ws: Option<&str>) -> Vec<u8> {
        let mut c = Vec::new();
        len_delim(1, uuid.as_bytes(), &mut c); // field 1 = uuid
        let inner = inner_summary(title, ws);
        let inner_b64 = base64::engine::general_purpose::STANDARD.encode(&inner);
        // The wrapper: field 1 = the base64 text of the inner Summary.
        let mut wrapper = Vec::new();
        len_delim(1, inner_b64.as_bytes(), &mut wrapper);
        len_delim(2, &wrapper, &mut c); // field 2 = wrapper protobuf
        c
    }
    /// The outer index, then base64 it (as stored in the vscdb value).
    fn index_b64(entries: &[(&str, &str, Option<&str>)]) -> Vec<u8> {
        let mut idx = Vec::new();
        for (uuid, title, ws) in entries {
            let c = entry(uuid, title, *ws);
            len_delim(1, &c, &mut idx); // top-level field 1 = repeated entry
        }
        base64::engine::general_purpose::STANDARD
            .encode(&idx)
            .into_bytes()
    }

    #[test]
    fn parse_trajectory_summaries_extracts_uuid_title_project() {
        // The nested base64-protobuf shape, byte-built exactly as the IDE stores
        // it: outer repeated entries, each with a uuid and a base64-wrapped inner
        // summary carrying the title + a file:// workspace.
        let bytes = index_b64(&[
            (
                "e141b632-e804-4481-8884-9a3452286fcf",
                "Bluey Landing Page Handoff",
                Some("file:///Users/ms/Developer/Bluey"),
            ),
            (
                "27845a77-5d00-42ac-8c51-ce24757a597c",
                "Testing Outreach OS Integration",
                Some("file:///Users/ms/Developer/Outreach%20OS"),
            ),
            // A conversation with no workspace (outside a project).
            (
                "00000000-0000-0000-0000-000000000000",
                "No Project Chat",
                None,
            ),
        ]);
        let summaries = parse_trajectory_summaries_bytes(&bytes);
        assert_eq!(summaries.len(), 3);
        assert_eq!(summaries[0].uuid, "e141b632-e804-4481-8884-9a3452286fcf");
        assert_eq!(summaries[0].title, "Bluey Landing Page Handoff");
        assert_eq!(
            summaries[0].project.as_deref(),
            Some("/Users/ms/Developer/Bluey"),
            "file:// scheme stripped"
        );
        assert_eq!(summaries[1].title, "Testing Outreach OS Integration");
        assert_eq!(
            summaries[1].project.as_deref(),
            Some("/Users/ms/Developer/Outreach OS"),
            "percent-escape (%20) decoded in the project path"
        );
        assert_eq!(summaries[2].title, "No Project Chat");
        assert_eq!(summaries[2].project, None, "no workspace → no project");
    }

    /// Build a synthetic IDE `state.vscdb` with an `ItemTable` carrying the
    /// `trajectorySummaries` row (and a decoy `oauthToken` row we must NOT read).
    fn make_vscdb(path: &Path, entries: &[(&str, &str, Option<&str>)]) {
        let conn = Connection::open(path).expect("open writable for setup");
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .expect("create table");
        let value = String::from_utf8(index_b64(entries)).expect("utf8 b64");
        conn.execute(
            "INSERT INTO ItemTable VALUES (?1, ?2)",
            rusqlite::params![TRAJECTORY_SUMMARIES_KEY, value],
        )
        .unwrap();
        // A secret row the reader must never touch.
        conn.execute(
            "INSERT INTO ItemTable VALUES (?1, ?2)",
            rusqlite::params!["antigravityUnifiedStateSync.oauthToken", "TOP-SECRET-TOKEN"],
        )
        .unwrap();
    }

    fn store_at(path: PathBuf) -> SessionStore {
        SessionStore {
            path,
            format: SessionFormat::AntigravityIdeIndex,
        }
    }

    #[test]
    fn list_reads_vscdb_index_and_falls_back_to_project_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_vscdb(
            &db,
            &[
                (
                    "11111111-1111-1111-1111-111111111111",
                    "Real IDE Title",
                    Some("file:///Users/ms/Developer/Bluey"),
                ),
                // Empty title → fall back to the project leaf ("Bluey session").
                (
                    "22222222-2222-2222-2222-222222222222",
                    "",
                    Some("file:///x/Bluey"),
                ),
            ],
        );

        let reader = AntigravityIdeReader;
        let refs = reader.list(&store_at(db), 50).expect("list");
        assert_eq!(refs.len(), 2);
        let real = refs
            .iter()
            .find(|r| r.id == "11111111-1111-1111-1111-111111111111")
            .unwrap();
        assert_eq!(real.title.as_deref(), Some("Real IDE Title"));
        assert_eq!(real.project.as_deref(), Some("/Users/ms/Developer/Bluey"));
        let empty = refs
            .iter()
            .find(|r| r.id == "22222222-2222-2222-2222-222222222222")
            .unwrap();
        assert_eq!(empty.title.as_deref(), Some("Bluey session"));
    }

    #[test]
    fn list_never_reads_the_oauth_token_row() {
        // Defense-in-depth: the listing must surface ONLY the trajectory titles,
        // never the sibling secret row's value, anywhere in its output.
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("state.vscdb");
        make_vscdb(
            &db,
            &[(
                "33333333-3333-3333-3333-333333333333",
                "Some Title",
                Some("file:///p/Proj"),
            )],
        );
        let refs = AntigravityIdeReader.list(&store_at(db), 50).expect("list");
        let dump = format!("{refs:?}");
        assert!(
            !dump.contains("TOP-SECRET-TOKEN"),
            "secret oauthToken value must never appear in the listing"
        );
    }

    #[test]
    fn list_missing_vscdb_yields_empty_not_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reader = AntigravityIdeReader;
        let refs = reader
            .list(&store_at(dir.path().join("nope.vscdb")), 50)
            .expect("list must not error");
        assert!(refs.is_empty());
    }

    #[test]
    fn garbled_value_degrades_to_empty_not_panic() {
        // A non-base64 / non-protobuf value must not panic.
        assert!(parse_trajectory_summaries_bytes(b"not base64 @@@@").is_empty());
        // Valid base64 of garbage protobuf bytes → empty, no panic.
        let g = base64::engine::general_purpose::STANDARD.encode([0xff, 0xff, 0xff, 0xff]);
        let _ = parse_trajectory_summaries_bytes(g.as_bytes()); // must not panic
    }

    #[test]
    fn read_resolves_body_from_ide_root_via_shared_reader() {
        // `read` delegates to the shared antigravity body reader rooted at
        // `$HOME/.gemini/antigravity-ide`. Point HOME at a tempdir holding a brain
        // transcript and assert the turn comes back. (Serialized HOME mutation is
        // acceptable in a single focused test.)
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        let id = "44444444-4444-4444-4444-444444444444";
        let brain = home
            .join(".gemini")
            .join("antigravity-ide")
            .join("brain")
            .join(id)
            .join(".system_generated")
            .join("logs");
        std::fs::create_dir_all(&brain).unwrap();
        std::fs::write(
            brain.join("transcript.jsonl"),
            "{\"type\":\"user_input\",\"content\":\"hello ide\"}\n",
        )
        .unwrap();

        let prev = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        let reader = AntigravityIdeReader;
        // The store path (index) is irrelevant for read(); bodies come from HOME.
        let t = reader
            .read(&store_at(home.join("ignored.vscdb")), id, 10)
            .expect("read");
        match prev {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        assert_eq!(t.turns.len(), 1);
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "hello ide");
    }
}
