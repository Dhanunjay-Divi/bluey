//! REAL end-to-end verification of the cross-meeting facts memory: downloads
//! the actual bge-small ONNX model (first run, ~35MB), embeds real meeting
//! facts, and verifies semantic retrieval — no mocks anywhere.
//!
//! `#[ignore]` because it needs network + ~35MB on first run:
//! `cargo test -p cue-daemon --features parakeet-stt,local-memory \
//!    --test facts_memory_real -- --ignored`
#![cfg(feature = "local-memory")]

use cue_daemon::memory::FactsMemory;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads the real embedding model (network, ~35MB first run)"]
async fn real_model_indexes_and_recalls_meeting_facts() {
    // Isolated data dir so the test never touches the user's real store.
    let tmp = std::env::temp_dir().join(format!("bluey-facts-real-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("tmp dir");
    std::env::set_var("BLUEY_DATA_DIR", &tmp);
    // Reuse an already-downloaded model when present (dev machine), else the
    // default dir under the temp data dir triggers a real first-run download.
    let paths = cue_core::app_paths::AppPaths::discover().expect("paths");

    let memory = FactsMemory::ensure(&paths)
        .await
        .expect("facts memory must come up with the real model");

    // Index REAL extracted-fact shaped items from two different meetings.
    memory
        .index_fact(
            "meeting-a",
            "[Decision] Shard the database by tenant id, not by region",
        )
        .await
        .expect("index");
    memory
        .index_fact(
            "meeting-a",
            "[Constraint] Checkout endpoint p99 latency must stay under 200ms",
        )
        .await
        .expect("index");
    memory
        .index_fact("meeting-b", "[Owner] Raj owns the payments service")
        .await
        .expect("index");

    // Semantic recall with a PARAPHRASED question (not keyword match).
    let hits = memory
        .search("how did we decide to split up the db", 3, None)
        .await
        .expect("search");
    assert!(!hits.is_empty(), "must recall something");
    assert!(
        hits[0].text.contains("tenant id"),
        "top hit must be the sharding decision, got: {}",
        hits[0].text
    );

    // Excluding the active meeting removes its facts from recall.
    let hits = memory
        .search("who owns payments", 3, Some("meeting-b"))
        .await
        .expect("search");
    assert!(
        hits.iter().all(|h| h.meeting_id != "meeting-b"),
        "active-meeting facts must be excluded"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
