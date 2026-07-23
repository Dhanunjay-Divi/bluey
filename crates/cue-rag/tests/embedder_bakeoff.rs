//! REAL bake-off: bge-small vs EmbeddingGemma-300M on Bluey's ACTUAL content
//! shape — short natural-language prose (ledger facts, meeting decisions, agent
//! reasoning prose). NOT code, NOT web text. Measures hit@1 on paraphrased,
//! keyword, and entity queries so the model choice is grounded in our data, not
//! a generic MTEB number.
//!
//! Needs both models on disk (bge under bluey/models/bge-small-en, gemma under
//! bluey/models/embeddinggemma-300m). Run:
//! `cargo test -p cue-rag --features local-embed --test embedder_bakeoff -- --ignored --nocapture`
#![cfg(feature = "local-embed")]

use std::path::PathBuf;

use cue_rag::{LocalBgeEmbedder, LocalGemmaEmbedder};

fn models_root() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/bluey/models")
}

/// The corpus: meeting-shaped prose (ledger facts + decisions + reasoning), the
/// exact content Bluey embeds. (session_id, prose).
fn corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        ("s1", "We decided to shard the Postgres database by tenant id, not by region, to keep tenants isolated."),
        ("s1", "Checkout endpoint p99 latency must stay under two hundred milliseconds after the tuning work."),
        ("s2", "Raj owns the payments service migration and its on-call rotation."),
        ("s2", "We agreed to freeze production deployments on Fridays to reduce weekend incidents."),
        ("s3", "Dana owns the authentication rollout including single sign-on for enterprise customers."),
        ("s3", "Ticket CUE-142 was moved to the next sprint because the design was not finalized."),
        ("s4", "The enterprise contract requires SOC2 compliance certification by the fourth quarter."),
        ("s4", "We chose gRPC for internal service calls and REST for the public API surface."),
        ("s5", "Priya is leading the billing rewrite kickoff starting next month."),
        ("s5", "We will adopt feature flags for the checkout redesign to ship incrementally."),
        ("s6", "The mobile app must keep supporting iOS 16 for at least another year."),
        ("s6", "Marco owns the incident postmortem for the June database outage."),
        ("s7", "We are moving the analytics pipeline from Spark to DuckDB to cut cost."),
        ("s7", "Vendor spend must stay under forty thousand dollars per quarter going forward."),
        ("s8", "The weekly retro moves to Friday mornings so more of the team can attend."),
    ]
}

/// HARD queries — the discriminating cases. Distant paraphrase, implicit
/// reference, and near-miss distractors (queries whose surface words point at
/// the WRONG doc but whose meaning points at the right one). These are where a
/// stronger model should separate from bge-small; the easy set tied 12/12.
fn queries() -> Vec<(&'static str, &'static str)> {
    vec![
        // Distant paraphrase (no shared keywords with the target).
        (
            "keeping one customer's data from bleeding into another's",
            "tenant id",
        ),
        ("stop shipping code right before the weekend", "freeze"),
        (
            "break the big launch into smaller safe pieces",
            "feature flags",
        ),
        ("cut our data infrastructure bill", "DuckDB"),
        ("make sure we pass the security audit in time", "SOC2"),
        // Implicit / indirect reference.
        ("who do I ask about login problems", "Dana"),
        ("who should review the money-moving code", "Raj"),
        ("what slipped to later", "CUE-142"),
        // Near-miss distractor (surface words collide with a WRONG doc).
        // "internal calls" appears in the gRPC doc; the ANSWER about who to page
        // is Raj/Marco — tests whether the model is fooled by lexical overlap.
        ("how fast must the buy button respond", "two hundred"),
        ("which old phone OS are we stuck supporting", "iOS 16"),
        (
            "what did we pick for talking between our own services",
            "gRPC",
        ),
        ("who is cleaning up after the outage", "Marco"),
    ]
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
    for i in 0..a.len().min(b.len()) {
        d += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        d / (na.sqrt() * nb.sqrt())
    }
}

/// PURE-COSINE hit@1 for one embedder, isolating it from the bge-tuned hybrid
/// pipeline (whose floor/BM25 are calibrated to bge and unfairly penalize other
/// score distributions — proven separately). Returns (hits, per-query top score).
fn cosine_hits(
    docs: &[Vec<f32>],
    q_embs: &[Vec<f32>],
    corp: &[(&str, &str)],
    qs: &[(&str, &str)],
) -> usize {
    let mut hits = 0;
    for (qi, (_, expected)) in qs.iter().enumerate() {
        let best = (0..docs.len())
            .max_by(|&i, &j| {
                cosine(&q_embs[qi], &docs[i])
                    .partial_cmp(&cosine(&q_embs[qi], &docs[j]))
                    .unwrap()
            })
            .unwrap();
        if corp[best].1.contains(expected) {
            hits += 1;
        }
    }
    hits
}

#[test]
#[ignore = "3-way real bake-off; needs bge + arctic-s + gemma ONNX models on disk"]
fn three_way_hit_at_1_on_meeting_prose() {
    let root = models_root();
    let corp = corpus();
    let qs = queries();

    // --- bge-small (current, 384-dim, ~100MB RAM) ---
    let bge = LocalBgeEmbedder::load(
        &root.join("bge-small-en/model_int8.onnx"),
        &root.join("bge-small-en/tokenizer.json"),
    )
    .expect("bge on disk");
    // --- arctic-embed-s (384-dim, ~100MB RAM, SAME bge path: BertModel+CLS+
    //     identical query prefix — a free-footprint swap candidate) ---
    let arctic = LocalBgeEmbedder::load(
        &root.join("arctic-embed-s/model_int8.onnx"),
        &root.join("arctic-embed-s/tokenizer.json"),
    )
    .expect("arctic-s on disk");
    // --- EmbeddingGemma-300M at 768 and MRL-256 (~350MB / ~300MB RAM) ---
    let gemma768 = LocalGemmaEmbedder::load(
        &root.join("embeddinggemma-300m/model_quantized.onnx"),
        &root.join("embeddinggemma-300m/tokenizer.json"),
        None,
    )
    .expect("gemma on disk");
    let gemma256 = LocalGemmaEmbedder::load(
        &root.join("embeddinggemma-300m/model_quantized.onnx"),
        &root.join("embeddinggemma-300m/tokenizer.json"),
        Some(256),
    )
    .expect("gemma256 on disk");

    // Embed the corpus (documents) once per model.
    let bge_d: Vec<_> = corp
        .iter()
        .map(|(_, t)| bge.embed_sync(t, false).unwrap())
        .collect();
    let arc_d: Vec<_> = corp
        .iter()
        .map(|(_, t)| arctic.embed_sync(t, false).unwrap())
        .collect();
    let g768_d: Vec<_> = corp
        .iter()
        .map(|(_, t)| gemma768.embed_sync(t, false).unwrap())
        .collect();
    let g256_d: Vec<_> = corp
        .iter()
        .map(|(_, t)| gemma256.embed_sync(t, false).unwrap())
        .collect();
    // Embed the queries once per model.
    let bge_q: Vec<_> = qs
        .iter()
        .map(|(q, _)| bge.embed_sync(q, true).unwrap())
        .collect();
    let arc_q: Vec<_> = qs
        .iter()
        .map(|(q, _)| arctic.embed_sync(q, true).unwrap())
        .collect();
    let g768_q: Vec<_> = qs
        .iter()
        .map(|(q, _)| gemma768.embed_sync(q, true).unwrap())
        .collect();
    let g256_q: Vec<_> = qs
        .iter()
        .map(|(q, _)| gemma256.embed_sync(q, true).unwrap())
        .collect();

    let n = qs.len();
    let bge_h = cosine_hits(&bge_d, &bge_q, &corp, &qs);
    let arc_h = cosine_hits(&arc_d, &arc_q, &corp, &qs);
    let g768_h = cosine_hits(&g768_d, &g768_q, &corp, &qs);
    let g256_h = cosine_hits(&g256_d, &g256_q, &corp, &qs);

    println!("\n=== 3-WAY PURE-COSINE hit@1 on HARD meeting-prose queries ===");
    println!("  bge-small     384d  ~100MB RAM :  {bge_h}/{n}");
    println!("  arctic-embed-s 384d ~100MB RAM :  {arc_h}/{n}   (free-footprint swap)");
    println!("  gemma-256      256d ~300MB RAM :  {g256_h}/{n}");
    println!("  gemma-768      768d ~350MB RAM :  {g768_h}/{n}");
    println!("\n  RAM-aware read: pick the best hit-rate whose RAM you'll pay for.");

    assert!(
        bge_h > 0 && arc_h > 0 && g768_h > 0,
        "harness sanity: all must hit something"
    );
}

#[test]
#[ignore = "measure Gemma's cosine score distribution to recalibrate the floor"]
fn gemma_score_distribution() {
    let root = models_root();
    let gemma = LocalGemmaEmbedder::load(
        &root.join("embeddinggemma-300m/model_quantized.onnx"),
        &root.join("embeddinggemma-300m/tokenizer.json"),
        None,
    )
    .expect("gemma");
    let cos = |a: &[f32], b: &[f32]| -> f32 {
        let (mut d, mut na, mut nb) = (0f32, 0f32, 0f32);
        for i in 0..a.len().min(b.len()) {
            d += a[i] * b[i];
            na += a[i] * a[i];
            nb += b[i] * b[i];
        }
        if na == 0.0 || nb == 0.0 {
            0.0
        } else {
            d / (na.sqrt() * nb.sqrt())
        }
    };
    let corp = corpus();
    let docs: Vec<Vec<f32>> = corp
        .iter()
        .map(|(_, t)| gemma.embed_sync(t, false).unwrap())
        .collect();
    println!("\n=== Gemma cosine: TOP-match score per query (vs bge floor 0.45) ===");
    let (mut min_correct, mut max_wrong) = (1.0f32, 0.0f32);
    for (q, expected) in queries() {
        let qe = gemma.embed_sync(q, true).unwrap();
        let mut scored: Vec<(f32, bool)> = docs
            .iter()
            .enumerate()
            .map(|(i, d)| (cos(&qe, d), corp[i].1.contains(expected)))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let top = scored[0];
        println!("  top={:.3} correct={}  {}", top.0, top.1, q);
        if top.1 {
            min_correct = min_correct.min(top.0);
        } else {
            max_wrong = max_wrong.max(top.0);
        }
    }
    println!(
        "\n  min correct-top score: {:.3}  (floor must be <= this to keep Gemma's hits)",
        min_correct
    );
    println!("  max wrong-top  score: {:.3}", max_wrong);
    println!(
        "  bge floor 0.45 keeps Gemma correct hits: {}",
        min_correct >= 0.45
    );
}
