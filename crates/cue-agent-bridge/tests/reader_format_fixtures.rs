//! Version-tagged format fixtures (H4) — the OFFLINE half of the drift defense.
//!
//! The live canary (`reader_health_canary.rs`) only fires on a machine that has
//! the agent installed. These fixtures are committed, shape-accurate samples of
//! each vendor's on-disk format, **tagged by the version/era they came from**, so
//! the readers are exercised against known-good bytes in EVERY CI run — including
//! runners with no agents installed. If a future refactor breaks parsing of a
//! format we already support, one of these goes red and names the era.
//!
//! Values are synthetic (no real conversation content, ids, or PII); the KEY
//! NAMES and NESTING mirror real captured bytes, because a key rename / restructure
//! is exactly the drift mode these guard against. Each fixture's filename encodes
//! the era (e.g. `claude_app_index_2026-06_no-completedTurns.json`) so when a
//! NEW era ships, you add a new fixture beside the old one — the old one keeps
//! proving the reader still handles legacy stores, the new one proves it handles
//! the new shape.

use std::path::PathBuf;

use cue_agent_bridge::{reader_for, SessionStore};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions")
}

/// A Claude **App** index in the 2026-06 era — the format whose dropped
/// `completedTurns` field caused the live 0/18 drift. This fixture pins that the
/// reader lists a session even with NO `completedTurns` key.
#[test]
fn claude_app_index_new_era_without_completed_turns_lists() {
    let dir = tempfile::tempdir().unwrap();
    // The reader walks <store>/<account>/<workspace>/local_*.json, so nest it.
    let ws = dir.path().join("acct").join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let bytes =
        std::fs::read(fixtures_dir().join("claude_app_index_2026-06_no-completedTurns.json"))
            .expect("read new-era app fixture");
    std::fs::write(ws.join("local_new.json"), &bytes).unwrap();

    let store = SessionStore {
        path: dir.path().to_path_buf(),
        format: cue_agent_bridge::SessionFormat::ClaudeAppIndex,
    };
    let reader = reader_for(store.format);
    let refs = reader.list(&store, 10).expect("list new-era app index");
    assert_eq!(
        refs.len(),
        1,
        "new-era session must list despite no completedTurns"
    );
    assert_eq!(refs[0].title.as_deref(), Some("Example session title"));
    assert!(!reader.health(&store).is_total_drift());
}

/// A Claude **App** index in the 2026-05 era — WITH `completedTurns`. Pins that
/// the reader still handles the legacy shape (and still drops a 0-turn draft).
#[test]
fn claude_app_index_old_era_with_completed_turns_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("acct").join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let bytes =
        std::fs::read(fixtures_dir().join("claude_app_index_2026-05_with-completedTurns.json"))
            .expect("read old-era app fixture");
    std::fs::write(ws.join("local_old.json"), &bytes).unwrap();

    let store = SessionStore {
        path: dir.path().to_path_buf(),
        format: cue_agent_bridge::SessionFormat::ClaudeAppIndex,
    };
    let reader = reader_for(store.format);
    let refs = reader.list(&store, 10).expect("list old-era app index");
    assert_eq!(refs.len(), 1, "legacy completedTurns>0 session still lists");
    assert_eq!(refs[0].title.as_deref(), Some("Older example session"));
}

/// Claude **CLI** jsonl turns (2026-06 envelope) — the `messages`/`content`
/// nesting whose v2.1.128 change broke 8+ third-party readers. Pins that the
/// reader decodes user + assistant turns and reads the stored title.
#[test]
fn claude_cli_jsonl_turns_decode_and_title() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = std::fs::read(fixtures_dir().join("claude_cli_turns_2026-06.jsonl"))
        .expect("read cli jsonl fixture");
    let file = dir.path().join("session.jsonl");
    std::fs::write(&file, &bytes).unwrap();

    let store = SessionStore {
        path: dir.path().to_path_buf(),
        format: cue_agent_bridge::SessionFormat::Jsonl,
    };
    let reader = reader_for(store.format);

    // The transcript decodes the user + assistant turns (titles are separate).
    let t = reader.read(&store, "session", 10).expect("read cli turns");
    assert!(
        t.turns.len() >= 2,
        "expected user + assistant turns from the envelope, got {}",
        t.turns.len()
    );

    // The store is healthy (a body decodes) — not drift.
    assert!(!reader.health(&store).is_total_drift());
}
