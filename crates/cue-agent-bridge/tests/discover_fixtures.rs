//! Integration tests for `cue-agent-bridge` Slice 1 — against SYNTHETIC
//! fixtures only. These tests never read the real user's `~/.cursor`, `~/.claude`,
//! or any real agent data. Fixture trees are built under `tempfile` temp dirs
//! and committed fixture files in `tests/fixtures/`.

use std::fs;
use std::path::{Path, PathBuf};

use cue_agent_bridge::{
    discover_in_home, probe_sqlite_store, read_connectors, AgentKind, AuthTier, Capability,
    Transport,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Build a minimal, valid SQLite `state.vscdb` at `path` (read/write only at
/// build time; discovery opens it read-only). Synthetic — no real data.
fn build_synthetic_vscdb(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create vscdb parent");
    }
    let conn = rusqlite::Connection::open(path).expect("create synthetic vscdb");
    conn.execute_batch(
        "CREATE TABLE ItemTable (key TEXT, value BLOB);
         INSERT INTO ItemTable VALUES ('synthetic', 'x');",
    )
    .expect("seed synthetic vscdb");
}

#[test]
fn reads_cursor_style_jsonc_config() {
    let conns = read_connectors(&fixture("cursor-mcp.jsonc"));
    assert_eq!(conns.len(), 3, "three servers in the fixture");

    let by_name = |n: &str| conns.iter().find(|c| c.name == n).cloned();

    let fsv = by_name("filesystem").expect("filesystem server");
    assert!(matches!(fsv.transport, Transport::Stdio { .. }));
    assert_eq!(fsv.auth_tier, AuthTier::None_);

    let pg = by_name("postgres").expect("postgres server");
    assert_eq!(pg.auth_tier, AuthTier::EnvAuth);

    let sb = by_name("supabase").expect("supabase server");
    assert!(matches!(sb.transport, Transport::Http { .. }));
    assert_eq!(sb.auth_tier, AuthTier::HostedOauth);
}

#[test]
fn never_exposes_env_secret_from_fixture() {
    let conns = read_connectors(&fixture("cursor-mcp.jsonc"));
    let dump = serde_json::to_string(&conns).expect("serialize");
    assert!(
        !dump.contains("redacted-not-read"),
        "env value must not surface"
    );
    assert!(!dump.contains("PG_URL"), "env key must not surface");
}

#[test]
fn missing_config_is_fail_soft() {
    let conns = read_connectors(Path::new("/nonexistent/zzz/mcp.json"));
    assert!(conns.is_empty(), "missing config yields empty, not panic");
}

#[test]
fn probes_real_sqlite_read_only() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("state.vscdb");
    build_synthetic_vscdb(&db);
    assert!(probe_sqlite_store(&db), "valid sqlite should probe true");

    // A non-SQLite file must not be mistaken for a store.
    let junk = tmp.path().join("not-a-db.vscdb");
    fs::write(&junk, b"this is not sqlite").expect("write junk");
    assert!(!probe_sqlite_store(&junk), "junk file must probe false");
}

#[test]
fn generic_vscode_fork_detector_finds_unknown_fork() {
    let home = tempfile::tempdir().expect("tempdir");
    let home_path = home.path();

    // An UNKNOWN fork: a dir under Application Support with the telltale vscdb,
    // but not in the registry. Must be detected as Other(name).
    let fork_dir = home_path.join("Library/Application Support/MysteryFork");
    build_synthetic_vscdb(&fork_dir.join("User/globalStorage/state.vscdb"));
    // A real VS Code-family fork keeps its chats under `User/workspaceStorage/`
    // (the global vscdb here has only `ItemTable`, NOT Cursor's `cursorDiskKV`).
    // Discovery keys the reader off store SHAPE (C6): an `ItemTable`-only vscdb
    // with a workspaceStorage dir is a `JsonFiles` store, not a `SqliteVscdb`
    // one — so create the dir so the fork has a readable store to record.
    fs::create_dir_all(fork_dir.join("User/workspaceStorage"))
        .expect("create fork workspaceStorage");
    // Give it an mcp config too.
    fs::write(
        fork_dir.join("User/mcp.json"),
        r#"{ "servers": { "x": { "command": "y" } } }"#,
    )
    .expect("write fork mcp");

    let agents = discover_in_home(home_path);
    let fork = agents
        .iter()
        .find(|a| matches!(&a.kind, AgentKind::Other(n) if n == "MysteryFork"))
        .expect("unknown fork detected");

    assert!(
        fork.session_store.is_some(),
        "fork should record its workspaceStorage chat store"
    );
    assert!(
        fork.connector_config_path.is_some(),
        "fork should record its mcp config"
    );
    // No CLI on PATH for it → read-only.
    assert_eq!(fork.capability, Capability::ReadOnly);
}

#[test]
fn known_fork_maps_to_registry_kind() {
    let home = tempfile::tempdir().expect("tempdir");
    let home_path = home.path();

    // "Cursor" is a registry data-dir name under Application Support.
    let cursor_dir = home_path.join("Library/Application Support/Cursor");
    build_synthetic_vscdb(&cursor_dir.join("User/globalStorage/state.vscdb"));

    let agents = discover_in_home(home_path);
    assert!(
        agents.iter().any(|a| a.kind == AgentKind::Cursor),
        "registry-named fork should map to Cursor, not Other"
    );
}

#[test]
fn claude_jsonl_footprint_is_read_only() {
    let home = tempfile::tempdir().expect("tempdir");
    let home_path = home.path();

    // ~/.claude with a projects dir (JSONL store) but no CLI on PATH.
    let projects = home_path.join(".claude/projects/demo");
    fs::create_dir_all(&projects).expect("mkdir claude projects");
    fs::copy(fixture("session.jsonl"), projects.join("a.jsonl")).expect("copy jsonl");

    let agents = discover_in_home(home_path);
    let claude = agents
        .iter()
        .find(|a| a.kind == AgentKind::ClaudeCode)
        .expect("claude footprint detected");
    assert_eq!(claude.capability, Capability::ReadOnly);
    assert!(claude.session_store.is_some());
}

#[test]
fn malformed_vscdb_degrades_not_panics() {
    let home = tempfile::tempdir().expect("tempdir");
    let home_path = home.path();

    // A dir that LOOKS like a fork but its vscdb is corrupt → must be skipped,
    // discovery continues without panic.
    let bad = home_path.join("Library/Application Support/BrokenFork");
    let bad_db = bad.join("User/globalStorage/state.vscdb");
    fs::create_dir_all(bad_db.parent().unwrap()).expect("mkdir broken");
    fs::write(&bad_db, b"corrupt-not-sqlite").expect("write corrupt");

    let agents = discover_in_home(home_path);
    assert!(
        !agents
            .iter()
            .any(|a| matches!(&a.kind, AgentKind::Other(n) if n == "BrokenFork")),
        "corrupt vscdb must not mint a fake agent"
    );
}

#[test]
fn empty_home_yields_no_home_rooted_agents() {
    // An empty synthetic home contributes no footprints. (The app-bundle pass
    // still scans the real `/Applications`, so we assert specifically that no
    // discovered agent draws evidence from this temp home, rather than that the
    // list is globally empty.)
    let home = tempfile::tempdir().expect("tempdir");
    let home_path = home.path();
    let agents = discover_in_home(home_path);
    assert!(
        !agents
            .iter()
            .any(|a| a.install_evidence.iter().any(|p| p.starts_with(home_path))),
        "empty home → no agent evidence rooted under it, no panic"
    );
}
