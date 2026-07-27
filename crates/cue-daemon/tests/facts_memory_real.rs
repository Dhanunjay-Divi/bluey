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

/// `BLUEY_DATA_DIR` is process-global env; parallel test threads racing
/// `set_var` → `AppPaths::discover` could cross-contaminate stores. Each test
/// holds this for its whole body (hence returned to the caller).
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn memory_in(
    tag: &str,
) -> (
    FactsMemory,
    std::path::PathBuf,
    tokio::sync::MutexGuard<'static, ()>,
) {
    let guard = ENV_LOCK.lock().await;
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
    (memory, tmp, guard)
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
    let (memory, tmp, _env) = memory_in("recall").await;

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

/// MEASURED retrieval eval: the hybrid pipeline (BM25 + entity boosts, the
/// mem0 v3 search port) against the pure-cosine baseline on a meeting-shaped
/// fact set — real embedder, labeled queries, hit@1 reported side by side.
/// The asserted gate is NO REGRESSION (hybrid >= cosine on hit@1); the
/// per-query table printed by the test is the evidence for where each
/// signal wins or ties.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real embedding model (network, ~35MB first run)"]
async fn hybrid_beats_cosine_baseline() {
    let (memory, tmp, _env) = memory_in("eval").await;

    let facts: &[(&str, &str)] = &[
        (
            "m1",
            "[Decision] Shard the database by tenant id, not by region",
        ),
        (
            "m1",
            "[Constraint] Checkout endpoint p99 latency must stay under 200ms",
        ),
        ("m1", "[Owner] Raj owns the payments service migration"),
        ("m1", "[Decision] Freeze production deployments on Fridays"),
        (
            "m2",
            "[Owner] Dana owns the authentication rollout including SSO",
        ),
        ("m2", "[Decision] Ticket CUE-142 moves to the next sprint"),
        (
            "m2",
            "[Constraint] The enterprise contract requires SOC2 compliance by Q4",
        ),
        (
            "m2",
            "[Decision] Use gRPC for internal service calls, REST for the public API",
        ),
        ("m2", "[Owner] Priya owns the billing rewrite kickoff"),
        (
            "m3",
            "[Decision] Adopt feature flags for the checkout redesign",
        ),
        (
            "m3",
            "[Constraint] The mobile app must keep supporting iOS 16",
        ),
        (
            "m3",
            "[Owner] Marco owns the incident postmortem for the June outage",
        ),
        (
            "m3",
            "[Decision] Move the analytics pipeline from Spark to DuckDB",
        ),
        (
            "m3",
            "[Constraint] Vendor spend must stay under 40k per quarter",
        ),
        ("m3", "[Decision] The retro moves to Friday mornings"),
    ];
    for (meeting, text) in facts {
        seed(&memory, meeting, &[text]).await;
    }

    // (query, substring the TOP hit must contain)
    let queries: &[(&str, &str)] = &[
        // Paraphrase (semantic strength — hybrid must not regress these).
        ("how did we decide to split up the db", "tenant id"),
        ("who is responsible for auth", "Dana"),
        (
            "what did we agree about deploying at the end of the week",
            "Freeze",
        ),
        // Keyword / identifier (BM25 strength).
        ("what happened to CUE-142", "CUE-142"),
        ("checkout latency SLA number", "200ms"),
        ("SOC2 deadline", "SOC2"),
        // Entity (boost strength).
        ("what is Raj working on", "Raj"),
        ("what does Marco own", "Marco"),
        ("Priya's project", "Priya"),
        // Mixed.
        ("Spark replacement decision", "DuckDB"),
        ("iOS support constraint", "iOS 16"),
        ("gRPC vs REST decision", "gRPC"),
    ];

    let (mut cosine_hits, mut hybrid_hits) = (0usize, 0usize);
    println!("\n{:<48} {:>8} {:>8}", "query", "cosine", "hybrid");
    for (query, expected) in queries {
        let baseline = memory
            .search_semantic_only(query, 3, None)
            .await
            .expect("baseline");
        let hybrid = memory.search(query, 3, None).await.expect("hybrid");
        let c = baseline
            .first()
            .map(|h| h.text.contains(expected))
            .unwrap_or(false);
        let h = hybrid
            .first()
            .map(|h| h.text.contains(expected))
            .unwrap_or(false);
        cosine_hits += c as usize;
        hybrid_hits += h as usize;
        println!(
            "{query:<48} {:>8} {:>8}",
            if c { "hit" } else { "MISS" },
            if h { "hit" } else { "MISS" }
        );
        if !h {
            for hit in &hybrid {
                println!(
                    "    hybrid: combined={:.3} semantic={:.3}  {}",
                    hit.score, hit.semantic, hit.text
                );
            }
            let ents = cue_rag::hybrid::extract_entities(query);
            println!("    query entities: {ents:?}");
        }
    }
    println!(
        "hit@1: cosine {cosine_hits}/{} vs hybrid {hybrid_hits}/{}",
        queries.len(),
        queries.len()
    );
    assert!(
        hybrid_hits >= cosine_hits,
        "hybrid must never regress the baseline: {hybrid_hits} < {cosine_hits}"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the real embedding model AND the claude CLI (drives a real one-shot)"]
async fn real_agent_update_phase_supersedes_reversed_decision() {
    let (memory, tmp, _env) = memory_in("update").await;

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
