//! REAL end-to-end verification of the cross-meeting facts memory: downloads
//! the actual bge-small ONNX model (first run, ~35MB), embeds real meeting
//! facts, runs the REAL Mem0 update phase (a real `claude -p` one-shot makes
//! the ADD/UPDATE/DELETE/NONE decision), and verifies semantic retrieval —
//! no mocks anywhere.
//!
//! `#[ignore]` because it needs network (~35MB first run) and, for the update
//! -phase test, the `claude` CLI:
//! `cargo test -p cue-daemon --features parakeet-stt,local-memory \
//!    --test facts_memory_real -- --ignored --nocapture`
#![cfg(feature = "local-memory")]

use cue_daemon::memory::FactsMemory;

async fn memory_in(tag: &str) -> (FactsMemory, std::path::PathBuf) {
    // Isolated data dir so the test never touches the user's real store.
    let tmp = std::env::temp_dir().join(format!("bluey-facts-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("tmp dir");
    std::env::set_var("BLUEY_DATA_DIR", &tmp);
    // Reuse an already-downloaded model when present (dev machine), else the
    // default dir under the temp data dir triggers a real first-run download.
    let paths = cue_core::app_paths::AppPaths::discover().expect("paths");
    let memory = FactsMemory::ensure(&paths)
        .await
        .expect("facts memory must come up with the real model");
    (memory, tmp)
}

/// Consolidate via the heuristic path (what the daemon does with no agent):
/// prepare (embed + dedup + neighborhood) then apply.
async fn seed(memory: &FactsMemory, meeting_id: &str, texts: &[&str]) {
    let candidates: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
    let plan = memory.prepare_update(&candidates).await.expect("prepare");
    memory
        .apply_heuristic(&plan, meeting_id)
        .await
        .expect("apply");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads the real embedding model (network, ~35MB first run)"]
async fn real_model_indexes_and_recalls_meeting_facts() {
    let (memory, tmp) = memory_in("recall").await;

    // Consolidate REAL extracted-fact shaped items from two meetings.
    seed(
        &memory,
        "meeting-a",
        &[
            "[Decision] Shard the database by tenant id, not by region",
            "[Constraint] Checkout endpoint p99 latency must stay under 200ms",
        ],
    )
    .await;
    seed(
        &memory,
        "meeting-b",
        &["[Owner] Raj owns the payments service"],
    )
    .await;

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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real embedding model AND the claude CLI (drives a real one-shot)"]
async fn real_agent_update_phase_supersedes_reversed_decision() {
    let (memory, tmp) = memory_in("update").await;

    // Meeting A established two facts.
    seed(
        &memory,
        "meeting-a",
        &[
            "[Decision] Shard the database by tenant id, not by region",
            "[Owner] Raj owns the payments service",
        ],
    )
    .await;

    // Meeting B REVERSES the sharding decision. prepare must surface the old
    // fact in the neighborhood and produce a decision prompt.
    let candidates =
        vec!["[Decision] Shard the database by customer region instead of tenant id".to_string()];
    let plan = memory.prepare_update(&candidates).await.expect("prepare");
    let prompt = plan
        .prompt()
        .expect("existing memory must trigger the decision prompt")
        .to_string();

    // THE REAL THING: a throwaway `claude -p` one-shot decides the ops —
    // the same headless print mode the daemon's drive uses.
    let output = std::process::Command::new("claude")
        .args(["-p", &prompt])
        .env_remove("ANTHROPIC_LOG") // keep stdout clean
        .output()
        .expect("claude CLI must be installed for this real test");
    assert!(
        output.status.success(),
        "claude -p failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    println!("agent decision:\n{raw}");

    let report = memory
        .apply_agent_ops(&plan, &raw, "meeting-b")
        .await
        .expect("agent ops must parse and apply");
    println!("report: {report:?}");
    assert!(
        report.updated + report.deleted + report.added > 0,
        "a reversed decision must produce a mutating op, got {report:?}"
    );

    // The stale decision must no longer be recallable as current truth.
    let hits = memory
        .search("how do we shard the database", 3, None)
        .await
        .expect("search");
    assert!(!hits.is_empty());
    assert!(
        hits[0].text.contains("region"),
        "current truth must be region sharding, got: {}",
        hits[0].text
    );
    assert!(
        !hits
            .iter()
            .any(|h| h.text.contains("tenant id") && !h.text.contains("region")),
        "the superseded tenant-id fact must not surface as current: {hits:?}"
    );

    // Unrelated fact untouched.
    let hits = memory
        .search("who owns the payments service", 3, None)
        .await
        .expect("search");
    assert!(hits[0].text.contains("Raj"));

    let _ = std::fs::remove_dir_all(&tmp);
}
