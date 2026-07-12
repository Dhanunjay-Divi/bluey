//! Thin facade over cue-rag for daemon-level RAG operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::warn;

use cue_core::{ContextArtifact, ContextProcessingStatus};
use cue_rag::{Chunker, EmbeddingProvider, RagHit, RagScope, VectorStore};

const MAX_CONTEXT_MARKDOWN_INDEX_CHARS: usize = 128_000;

/// Daemon-level RAG pipeline: chunker + embedder + vector store.
pub struct RagPipeline {
    chunker: Chunker,
    embedder: Arc<dyn EmbeddingProvider>,
    store: Arc<Mutex<VectorStore>>,
    scope: RagScope,
}

impl RagPipeline {
    /// Create a new RAG pipeline.
    pub fn new(
        store_path: PathBuf,
        embedder: Arc<dyn EmbeddingProvider>,
        scope: RagScope,
    ) -> Result<Self> {
        let dim = embedder.dim();
        let store = VectorStore::open(&store_path, dim)?;
        Ok(Self {
            chunker: Chunker::new(),
            embedder,
            store: Arc::new(Mutex::new(store)),
            scope,
        })
    }

    pub(crate) fn scope(&self) -> &RagScope {
        &self.scope
    }

    /// Index a transcript segment. Chunks the text, embeds each chunk, and stores.
    /// Errors are logged but never propagated (fire-and-forget for live indexing).
    pub async fn index_transcript(&self, session_id: &str, text: &str) {
        self.index_text(
            session_id,
            text,
            RagIndexSource {
                kind: "transcript",
                id: "",
                title: "",
                path: "",
            },
        )
        .await;
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
        let Some(body) = artifact_context_text(session_id, artifact) else {
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

        let artifact_kind = artifact.kind.to_string();
        let artifact_id = artifact.id.to_string();
        self.index_text(
            session_id,
            &text,
            RagIndexSource {
                kind: &artifact_kind,
                id: &artifact_id,
                title: &artifact.title,
                path: &artifact.path,
            },
        )
        .await;
    }

    async fn index_text(&self, session_id: &str, text: &str, source: RagIndexSource<'_>) {
        let chunks = self.chunker.chunk(text);
        if chunks.is_empty() {
            return;
        }
        let chunk_count = chunks.len();
        let text_chars = text.chars().count();

        let texts = chunks
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>();
        match self.embedder.embed_batch(&texts).await {
            Ok(embeddings) => {
                if embeddings.len() != chunks.len() {
                    warn!(
                        session_id,
                        source_kind = source.kind,
                        source_id = source.id,
                        source_title = source.title,
                        source_path = source.path,
                        chunks = chunk_count,
                        text_chars,
                        expected = chunks.len(),
                        actual = embeddings.len(),
                        "RAG embedding batch returned the wrong number of vectors"
                    );
                    return;
                }
                for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
                    let store = self.store.lock().await;
                    if let Err(e) = store.index(&self.scope, session_id, chunk, embedding) {
                        warn!(
                            session_id,
                            source_kind = source.kind,
                            source_id = source.id,
                            source_title = source.title,
                            source_path = source.path,
                            chunk_start = chunk.start_char,
                            chunks = chunk_count,
                            text_chars,
                            "RAG index store error: {e}"
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    session_id,
                    source_kind = source.kind,
                    source_id = source.id,
                    source_title = source.title,
                    source_path = source.path,
                    chunks = chunk_count,
                    text_chars,
                    "RAG embedding error: {e}"
                );
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
        store.query(&self.scope, &query_embedding, limit, session_id)
    }

    /// Query current-session and global memory using one embedding request.
    pub async fn query_current_and_global(
        &self,
        query_text: &str,
        current_limit: usize,
        current_session_id: &str,
        global_limit: usize,
    ) -> Result<(Vec<RagHit>, Vec<RagHit>)> {
        let query_embedding = self
            .embedder
            .embed(query_text)
            .await
            .map_err(|e| anyhow::anyhow!("embedding query failed: {e}"))?;
        let store = self.store.lock().await;
        let current = store.query(
            &self.scope,
            &query_embedding,
            current_limit,
            Some(current_session_id),
        )?;
        let global = store.query(&self.scope, &query_embedding, global_limit, None)?;
        Ok((current, global))
    }

    /// Delete all indexed data for a session.
    pub async fn delete_session(&self, session_id: &str) -> Result<usize> {
        let store = self.store.lock().await;
        store.delete_session(&self.scope, session_id)
    }

    /// Claim pre-account-scope rows after the coordinator verifies ownership.
    pub async fn claim_legacy_session(&self, session_id: &str) -> Result<usize> {
        let store = self.store.lock().await;
        store.claim_legacy_session(&self.scope, session_id)
    }
}

struct RagIndexSource<'a> {
    kind: &'a str,
    id: &'a str,
    title: &'a str,
    path: &'a str,
}

fn artifact_context_text(session_id: &str, artifact: &ContextArtifact) -> Option<String> {
    if let Some(path) = artifact
        .markdown_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        match read_text_prefix(Path::new(path), MAX_CONTEXT_MARKDOWN_INDEX_CHARS) {
            Ok(markdown) if !markdown.trim().is_empty() => return Some(markdown),
            Ok(_) => {}
            Err(error) => warn!(
                session_id,
                artifact_id = %artifact.id,
                artifact_kind = %artifact.kind,
                artifact_title = %artifact.title,
                source_path = %artifact.path,
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
