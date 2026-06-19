//! Thin facade over cue-rag for daemon-level RAG operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::warn;

use cue_core::{ContextArtifact, ContextProcessingStatus};
use cue_rag::{Chunker, EmbeddingProvider, RagHit, VectorStore};

const MAX_CONTEXT_MARKDOWN_INDEX_CHARS: usize = 128_000;

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
        self.index_text(session_id, text).await;
    }

    /// Index a user-approved context artifact such as a document, page capture,
    /// screenshot OCR/vision note, or source file preview.
    ///
    /// The vector store does not yet have separate source metadata columns, so
    /// the indexed text is prefixed with a compact source header. That keeps
    /// retrieved snippets self-explanatory in answer prompts and cloud sync.
    pub async fn index_context_artifact(&self, session_id: &str, artifact: &ContextArtifact) {
        if artifact.processing_status != ContextProcessingStatus::Ready {
            return;
        }
        let Some(body) = artifact_context_text(artifact) else {
            return;
        };

        let mut text = format!(
            "Attached context: {}\nKind: {}\nSource: {}",
            artifact.title, artifact.kind, artifact.path
        );
        if let Some(note) = artifact
            .note
            .as_deref()
            .filter(|note| !note.trim().is_empty())
        {
            text.push_str("\nNote: ");
            text.push_str(note.trim());
        }
        text.push_str("\n\n");
        text.push_str(body.trim());

        self.index_text(session_id, &text).await;
    }

    async fn index_text(&self, session_id: &str, text: &str) {
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

fn artifact_context_text(artifact: &ContextArtifact) -> Option<String> {
    if let Some(path) = artifact
        .markdown_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        match read_text_prefix(Path::new(path), MAX_CONTEXT_MARKDOWN_INDEX_CHARS) {
            Ok(markdown) if !markdown.trim().is_empty() => return Some(markdown),
            Ok(_) => {}
            Err(error) => warn!(
                artifact_id = %artifact.id,
                path,
                "failed to read local Markdown artifact for RAG indexing: {error:#}"
            ),
        }
    }

    artifact
        .text_preview
        .as_deref()
        .filter(|preview| !preview.trim().is_empty())
        .map(ToOwned::to_owned)
}

fn read_text_prefix(path: &Path, max_chars: usize) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let mut chars = text.chars();
    let mut prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        prefix.push_str("\n\n...");
    }
    Ok(prefix)
}
