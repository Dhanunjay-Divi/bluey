//  Benchmark / scaling test for VectorStore::query.
//!
//! Confirms the heap-based top-k path stays sub-second at realistic v0.1
//! per-user RAG sizes and gives us a regression baseline before any
//! sqlite-vec / usearch migration.

use cue_rag::store::VectorStore;
use cue_rag::{Chunk, RagScope};
use std::path::Path;
use std::time::Instant;

fn make_emb(seed: f32, dim: usize) -> Vec<f32> {
    (0..dim).map(|i| (i as f32 * seed).sin()).collect()
}

fn scope() -> RagScope {
    RagScope::new("scaling-test-account", Some("default")).unwrap()
}

#[test]
fn query_scales_under_one_second_at_ten_thousand_chunks() {
    // Realistic per-user RAG: 10_000 chunks, 1536-dim OpenAI embeddings.
    // The heap-based top-k must complete in well under 1s on uno (Apple
    // Silicon). We assert <2s as a generous CI ceiling.
    let dim = 1536;
    let n = 10_000;
    let limit = 10;

    let store = VectorStore::open(Path::new(":memory:"), dim).unwrap();
    let scope = scope();

    let index_start = Instant::now();
    for i in 0..n {
        let chunk = Chunk {
            text: format!("chunk {i}"),
            start_char: i * 10,
            end_char: (i + 1) * 10,
        };
        let emb = make_emb((i as f32 + 1.0) * 0.001, dim);
        store.index(&scope, "session-1", &chunk, &emb).unwrap();
    }
    let index_elapsed = index_start.elapsed();

    let query_emb = make_emb(0.5, dim);
    let q_start = Instant::now();
    let hits = store.query(&scope, &query_emb, limit, None).unwrap();
    let q_elapsed = q_start.elapsed();

    eprintln!(
        "rag bench: indexed {n} chunks in {:?}, queried in {:?} (limit={limit})",
        index_elapsed, q_elapsed
    );

    assert_eq!(hits.len(), limit);
    // Returned in score-descending order.
    for w in hits.windows(2) {
        assert!(w[0].score >= w[1].score);
    }
    assert!(
        q_elapsed.as_secs_f64() < 2.0,
        "query too slow at N=10000: {:?}",
        q_elapsed
    );
}

#[test]
fn empty_limit_returns_empty() {
    let store = VectorStore::open(Path::new(":memory:"), 4).unwrap();
    let scope = scope();
    let chunk = Chunk {
        text: "x".into(),
        start_char: 0,
        end_char: 1,
    };
    store
        .index(&scope, "s1", &chunk, &[1.0, 0.0, 0.0, 0.0])
        .unwrap();
    let hits = store.query(&scope, &[1.0, 0.0, 0.0, 0.0], 0, None).unwrap();
    assert!(hits.is_empty());
}

#[test]
fn limit_larger_than_corpus_returns_corpus_size() {
    let store = VectorStore::open(Path::new(":memory:"), 4).unwrap();
    let scope = scope();
    for i in 0..3 {
        store
            .index(
                &scope,
                "s",
                &Chunk {
                    text: format!("{i}"),
                    start_char: 0,
                    end_char: 1,
                },
                &make_emb((i as f32 + 1.0) * 0.5, 4),
            )
            .unwrap();
    }
    let hits = store.query(&scope, &make_emb(0.5, 4), 100, None).unwrap();
    assert_eq!(hits.len(), 3);
}
