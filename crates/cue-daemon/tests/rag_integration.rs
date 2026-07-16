//! Integration tests for the RAG pipeline.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use cue_core::{ContextArtifact, ContextKind};
use cue_daemon::db::rag::RagPipeline;
use cue_rag::{Chunk, Chunker, EmbeddingError, EmbeddingProvider, RagScope, VectorStore};

fn test_scope() -> RagScope {
    RagScope::new("integration-test-account", Some("default")).unwrap()
}

/// Mock embedder that returns deterministic embeddings based on text length.
struct MockEmbedder;

#[async_trait]
impl EmbeddingProvider for MockEmbedder {
    fn name(&self) -> &'static str {
        "mock"
    }
    fn dim(&self) -> usize {
        4
    }
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        // Deterministic embedding based on text content
        let len = text.len() as f32;
        Ok(vec![
            len.sin(),
            len.cos(),
            (len * 0.5).sin(),
            (len * 0.5).cos(),
        ])
    }
}

#[test]
fn chunker_basic() {
    let text = "A".repeat(2000);
    let chunker = Chunker::new();
    let chunks = chunker.chunk(&text);
    assert!(
        chunks.len() >= 3,
        "expected >=3 chunks, got {}",
        chunks.len()
    );
    for c in &chunks {
        assert!(c.text.len() <= chunker.max_chars);
    }
    // Verify overlap
    for i in 1..chunks.len() {
        assert!(chunks[i].start_char < chunks[i - 1].end_char);
    }
}

#[test]
fn chunker_short_text() {
    let chunker = Chunker::new();
    let chunks = chunker.chunk("Hello, world!");
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].text, "Hello, world!");
}

#[test]
fn vector_store_index_and_query() {
    let dim = 4;
    let store = VectorStore::open(Path::new(":memory:"), dim).unwrap();
    let scope = test_scope();

    for i in 0..5 {
        let chunk = Chunk {
            text: format!("chunk number {i} with some content"),
            start_char: i * 100,
            end_char: (i + 1) * 100,
        };
        // Each chunk gets a unique embedding
        let emb = vec![
            (i as f32 * 0.7).sin(),
            (i as f32 * 0.7).cos(),
            (i as f32 * 0.3).sin(),
            (i as f32 * 0.3).cos(),
        ];
        store.index(&scope, "session-A", &chunk, &emb).unwrap();
    }

    // Query with embedding identical to chunk 3
    let query = vec![
        (3.0_f32 * 0.7).sin(),
        (3.0_f32 * 0.7).cos(),
        (3.0_f32 * 0.3).sin(),
        (3.0_f32 * 0.3).cos(),
    ];
    let results = store.query(&scope, &query, 3, None).unwrap();
    assert_eq!(results.len(), 3);
    // Top result should be chunk 3 (exact match = score 1.0)
    assert!(
        results[0].score > 0.99,
        "top score should be ~1.0, got {}",
        results[0].score
    );
    assert!(results[0].chunk_text.contains("chunk number 3"));
    // Ordering by score
    assert!(results[0].score >= results[1].score);
    assert!(results[1].score >= results[2].score);
}

#[test]
fn vector_store_session_filter() {
    let dim = 4;
    let store = VectorStore::open(Path::new(":memory:"), dim).unwrap();
    let scope = test_scope();
    let emb = vec![1.0, 0.0, 0.0, 0.0];

    store
        .index(
            &scope,
            "s1",
            &Chunk {
                text: "s1 data".into(),
                start_char: 0,
                end_char: 7,
            },
            &emb,
        )
        .unwrap();
    store
        .index(
            &scope,
            "s2",
            &Chunk {
                text: "s2 data".into(),
                start_char: 0,
                end_char: 7,
            },
            &emb,
        )
        .unwrap();
    store
        .index(
            &scope,
            "s2",
            &Chunk {
                text: "s2 more".into(),
                start_char: 7,
                end_char: 14,
            },
            &emb,
        )
        .unwrap();

    // Filter to s1 only
    let results = store.query(&scope, &emb, 10, Some("s1")).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, "s1");

    // Filter to s2
    let results = store.query(&scope, &emb, 10, Some("s2")).unwrap();
    assert_eq!(results.len(), 2);

    // No filter returns all
    let results = store.query(&scope, &emb, 10, None).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn vector_store_delete_session_cascade() {
    let dim = 4;
    let store = VectorStore::open(Path::new(":memory:"), dim).unwrap();
    let scope = test_scope();
    let emb = vec![0.5, 0.5, 0.5, 0.5];

    store
        .index(
            &scope,
            "keep",
            &Chunk {
                text: "keeper".into(),
                start_char: 0,
                end_char: 6,
            },
            &emb,
        )
        .unwrap();
    store
        .index(
            &scope,
            "remove",
            &Chunk {
                text: "goner1".into(),
                start_char: 0,
                end_char: 6,
            },
            &emb,
        )
        .unwrap();
    store
        .index(
            &scope,
            "remove",
            &Chunk {
                text: "goner2".into(),
                start_char: 6,
                end_char: 12,
            },
            &emb,
        )
        .unwrap();

    assert_eq!(store.chunk_count(&scope).unwrap(), 3);
    let deleted = store.delete_session(&scope, "remove").unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(store.chunk_count(&scope).unwrap(), 1);

    // Query should only return the kept session
    let results = store.query(&scope, &emb, 10, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].session_id, "keep");
}

#[tokio::test]
async fn rag_live_indexing_smoke() {
    // Simulate the live indexing path: text -> chunker -> embedder -> store
    let dim = 4;
    let store = VectorStore::open(Path::new(":memory:"), dim).unwrap();
    let scope = test_scope();
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbedder);
    let chunker = Chunker::new();

    let transcript = "The quarterly results show a 15% increase in revenue.                       The team discussed the new product launch timeline and agreed                       on a March deadline. Action item: prepare the marketing materials.";

    let chunks = chunker.chunk(transcript);
    assert!(!chunks.is_empty());

    for chunk in &chunks {
        let embedding = embedder.embed(&chunk.text).await.unwrap();
        store
            .index(&scope, "live-session", chunk, &embedding)
            .unwrap();
    }

    // Verify data was indexed
    assert!(store.chunk_count(&scope).unwrap() > 0);

    // Query should return results
    let query_emb = embedder.embed("revenue increase").await.unwrap();
    let results = store
        .query(&scope, &query_emb, 5, Some("live-session"))
        .unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].session_id, "live-session");
}

#[tokio::test]
async fn rag_indexes_context_artifact_with_source_labels() {
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbedder);
    let pipeline = RagPipeline::new(PathBuf::from(":memory:"), embedder, test_scope()).unwrap();
    let artifact = ContextArtifact::new(
        ContextKind::Document,
        "/Users/example/launch-plan.md",
        "Launch plan",
        Some("Added during the live session".to_string()),
        Some(128),
    )
    .with_text_preview("The launch plan says revenue reporting depends on cache invalidation.");

    pipeline
        .index_context_artifact("session-docs", &artifact)
        .await
        .unwrap();

    let hits = pipeline
        .query("cache invalidation revenue", 5, Some("session-docs"))
        .await
        .unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].session_id, "session-docs");
    assert!(hits[0].chunk_text.contains("Attached context: Launch plan"));
    assert!(hits[0]
        .chunk_text
        .contains(&format!("Artifact ID: {}", artifact.id)));
    assert!(!hits[0].chunk_text.contains("/Users/example"));
    assert!(hits[0].chunk_text.contains("cache invalidation"));
}

#[tokio::test]
async fn rag_indexes_saved_markdown_artifact_when_available() {
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(MockEmbedder);
    let pipeline = RagPipeline::new(PathBuf::from(":memory:"), embedder, test_scope()).unwrap();
    let base = std::env::temp_dir().join(format!("bluey-rag-markdown-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let markdown_path = base.join("context.md");
    std::fs::write(
        &markdown_path,
        "# Deep Context\n\nThe converted markdown mentions quasar ledger reconciliation.",
    )
    .unwrap();

    let artifact = ContextArtifact::new(
        ContextKind::Document,
        "/Users/example/spec.pdf",
        "Spec",
        None,
        Some(128),
    )
    .with_text_preview("short preview without the searchable phrase")
    .with_markdown_path(markdown_path.display().to_string());

    pipeline
        .index_context_artifact("session-markdown", &artifact)
        .await
        .unwrap();

    let hits = pipeline
        .query("quasar ledger reconciliation", 5, Some("session-markdown"))
        .await
        .unwrap();
    assert!(hits
        .iter()
        .any(|hit| hit.chunk_text.contains("quasar ledger reconciliation")));

    let _ = std::fs::remove_dir_all(base);
}
