//! Security/bounds audit for the session readers (H5).
//!
//! These readers parse ARBITRARY local bytes — a corrupt store, a half-written
//! file, or a deliberately hostile one (the on-disk format is not a trust
//! boundary the user controls per-byte). A reader must therefore degrade, never
//! OOM, stack-overflow, hang, or panic on adversarial input. The production
//! readers are already bounded (file-size caps, line caps, recursion-depth caps,
//! serde_json's built-in 128-deep nesting guard, checked protobuf arithmetic).
//! This test PROVES those bounds hold against concrete attacks, so a future edit
//! that removes a cap is caught here rather than in the field.
//!
//! Every assertion is "returns Ok/empty, does not panic or hang" — the readers
//! are fail-soft, so the correct behavior on garbage is to surface nothing for
//! that record and keep going.

use cue_agent_bridge::{reader_for, SessionFormat, SessionStore};

fn store_at(path: std::path::PathBuf, format: SessionFormat) -> SessionStore {
    SessionStore { path, format }
}

#[test]
fn jsonl_deeply_nested_json_does_not_stack_overflow() {
    // A "nesting bomb": one line with thousands of open brackets. serde_json's
    // recursion limit must reject it as a parse error (skipped line), not blow
    // the stack. The reader keeps a valid line that follows.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bomb.jsonl");
    let bomb = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let good = r#"{"type":"user","message":{"role":"user","content":"ok"}}"#;
    std::fs::write(&file, format!("{bomb}\n{good}\n")).unwrap();

    let reader = reader_for(SessionFormat::Jsonl);
    let store = store_at(dir.path().to_path_buf(), SessionFormat::Jsonl);
    // Must not panic / overflow. The bomb line is skipped; the good line decodes.
    let t = reader
        .read(&store, "bomb", 10)
        .expect("read must not error");
    assert_eq!(t.turns.len(), 1, "bomb line skipped, good line kept");
}

#[test]
fn jsonl_giant_single_line_is_skipped_not_slurped() {
    // A single line far larger than MAX_LINE_BYTES must be skipped without
    // buffering it into a turn. (BufRead::lines still reads the line, but the
    // reader's length guard drops it before JSON parsing / allocation of a Turn.)
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("huge.jsonl");
    // 8 MB line (> the 4 MB MAX_LINE_BYTES), then a valid small line.
    let huge = format!(r#"{{"type":"user","x":"{}"}}"#, "a".repeat(8 * 1024 * 1024));
    let good = r#"{"type":"user","message":{"role":"user","content":"ok"}}"#;
    std::fs::write(&file, format!("{huge}\n{good}\n")).unwrap();

    let reader = reader_for(SessionFormat::Jsonl);
    let store = store_at(dir.path().to_path_buf(), SessionFormat::Jsonl);
    let t = reader
        .read(&store, "huge", 10)
        .expect("read must not error");
    assert_eq!(t.turns.len(), 1, "oversized line dropped, good line kept");
}

#[test]
fn jsonl_binary_garbage_yields_no_turns_no_panic() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("garbage.jsonl");
    std::fs::write(&file, [0u8, 159, 146, 150, 255, 254, 0, 10, 1, 2, 3]).unwrap();

    let reader = reader_for(SessionFormat::Jsonl);
    let store = store_at(dir.path().to_path_buf(), SessionFormat::Jsonl);
    let t = reader
        .read(&store, "garbage", 10)
        .expect("no error on binary");
    assert!(t.turns.is_empty(), "binary garbage yields no turns");
    // And health does not panic on it.
    let _ = reader.health(&store);
}

#[test]
fn antigravity_truncated_and_hostile_protobuf_is_fail_soft() {
    // Truncated varints, a length-delimited field claiming a huge length, and
    // random bytes must all parse to an empty/short list without panicking or
    // over-reading (the protobuf reader uses checked arithmetic + bounds checks).
    let dir = tempfile::tempdir().unwrap();
    let index = dir.path().join("agyhub_summaries_proto.pb");

    // Field 1, wire 2 (length-delimited), then a varint length of u64::MAX-ish,
    // then nothing — len_delim must reject the over-long claim, not allocate.
    let hostile: Vec<u8> = vec![
        0x0A, // tag: field 1, wire 2
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F, // varint ~ huge length
    ];
    std::fs::write(&index, &hostile).unwrap();

    let reader = reader_for(SessionFormat::AntigravityIndex);
    let store = store_at(index.clone(), SessionFormat::AntigravityIndex);
    // Must not panic / hang / over-read.
    let refs = reader.list(&store, 100).expect("list must not error");
    assert!(refs.is_empty(), "hostile protobuf yields no sessions");
    let _ = reader.health(&store);

    // Pure random-ish bytes too.
    std::fs::write(&index, vec![0xDE, 0xAD, 0xBE, 0xEF, 0x08, 0x96, 0x01]).unwrap();
    let refs = reader.list(&store, 100).expect("list must not error");
    assert!(refs.is_empty());
}

#[test]
fn readers_handle_a_nonexistent_store_without_panic() {
    // A store path that doesn't exist must be EmptyStore / empty list, never a
    // panic — discovery can race a store being deleted between find and read.
    let missing = std::path::PathBuf::from("/nonexistent/cue/store/path");
    for format in [
        SessionFormat::Jsonl,
        SessionFormat::JsonFiles,
        SessionFormat::AntigravityIndex,
        SessionFormat::ClaudeAppIndex,
        SessionFormat::SqliteVscdb,
    ] {
        let reader = reader_for(format);
        let store = store_at(missing.clone(), format);
        // list() may Ok(empty) or Err, but must NOT panic.
        let _ = reader.list(&store, 10);
        // health() must classify a missing store as EmptyStore (not drift).
        let h = reader.health(&store);
        assert!(
            !h.is_total_drift(),
            "missing store must not read as drift for {format:?}: {h:?}"
        );
    }
}
