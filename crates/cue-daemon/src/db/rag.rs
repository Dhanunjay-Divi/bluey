//! Thin facade over cue-rag for daemon-level RAG operations.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::warn;

use cue_rag::{Chunker, EmbeddingProvider, RagHit, VectorStore};

/// Daemon-level RAG pipeline: chunker + embedder + vector store.
pub struct RagPipeline {
    chunker: Chunker,
    embedder: Arc<dyn EmbeddingProvider>,
    store: Arc<Mutex<VectorStore>>,
}

impl RagPipeline {
    /// Create a new RAG pipeline.
    pub fn new(store_path: PathBuf, embedder: Arc<dyn EmbeddingProvider>) -> Result<Self> {
        let dim = embedder.dim();
        let store = VectorStore::open(&store_path, dim)?;
        Ok(Self {
            chunker: Chunker::new(),
            embedder,
            store: Arc::new(Mutex::new(store)),
        })
    }

    /// Index a transcript segment. Chunks the text, embeds each chunk, and stores.
    /// Errors are logged but never propagated (fire-and-forget for live indexing).
    pub async fn index_transcript(&self, session_id: &str, text: &str) {
        let chunks = self.chunker.chunk(text);
        if chunks.is_empty() {
            return;
        }

        for chunk in &chunks {
            match self.embedder.embed(&chunk.text).await {
                Ok(embedding) => {
                    let store = self.store.lock().await;
                    if let Err(e) = store.index(session_id, chunk, &embedding) {
                        warn!("RAG index store error: {e}");
                    }
                }
                Err(e) => {
                    warn!("RAG embedding error: {e}");
                }
            }
        }
    }

    /// Query the vector store for relevant chunks.
    pub async fn query(
        &self,
        query_text: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<RagHit>> {
        let query_embedding = self
            .embedder
            .embed(query_text)
            .await
            .map_err(|e| anyhow::anyhow!("embedding query failed: {e}"))?;
        let store = self.store.lock().await;
        store.query(&query_embedding, limit, session_id)
    }

    /// Delete all indexed data for a session.
    pub async fn delete_session(&self, session_id: &str) -> Result<usize> {
        let store = self.store.lock().await;
        store.delete_session(session_id)
    }
}
