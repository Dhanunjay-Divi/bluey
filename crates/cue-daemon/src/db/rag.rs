//! Thin facade over cue-rag for daemon-level RAG operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
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
        let store = VectorStore::open_for_provider(&store_path, embedder.as_ref())?;
        store.delete_staging_sessions(&scope)?;
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

    /// Index transcript-derived text. Failures propagate to the durable queue
    /// so they can be retried instead of disappearing in a detached task.
    pub async fn index_transcript(&self, session_id: &str, text: &str) -> Result<()> {
        self.index_text(
            session_id,
            text,
            RagIndexSource {
                kind: "transcript",
                id: "",
            },
        )
        .await
    }

    /// Index a user-approved context artifact such as a document, page capture,
    /// screenshot OCR/vision note, or source file preview.
    ///
    /// The vector store does not yet have separate source metadata columns, so
    /// the indexed text is prefixed with a compact source header. That keeps
    /// retrieved snippets self-explanatory in answer prompts and cloud sync.
    pub async fn index_context_artifact(
        &self,
        session_id: &str,
        artifact: &ContextArtifact,
    ) -> Result<()> {
        if artifact.processing_status != ContextProcessingStatus::Ready {
            return Ok(());
        }
        let Some(body) = artifact_context_text(artifact)? else {
            return Ok(());
        };

        let text = artifact_index_text(artifact, &body);

        let artifact_kind = artifact.kind.to_string();
        let artifact_id = artifact.id.to_string();
        self.index_text(
            session_id,
            &text,
            RagIndexSource {
                kind: &artifact_kind,
                id: &artifact_id,
            },
        )
        .await
    }

    async fn index_text(
        &self,
        session_id: &str,
        text: &str,
        source: RagIndexSource<'_>,
    ) -> Result<()> {
        let chunks = self.chunker.chunk(text);
        if chunks.is_empty() {
            return Ok(());
        }

        let texts = chunks
            .iter()
            .map(|chunk| chunk.text.clone())
            .collect::<Vec<_>>();
        let embeddings = self
            .embedder
            .embed_batch(&texts)
            .await
            .map_err(|error| anyhow::anyhow!("RAG embedding failed: {error}"))?;
        anyhow::ensure!(
            embeddings.len() == chunks.len(),
            "RAG embedding count mismatch for {} source {}",
            source.kind,
            source.id
        );

        let store = self.store.lock().await;
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            store
                .index(&self.scope, session_id, chunk, embedding)
                .with_context(|| {
                    format!(
                        "store RAG {} source {} at chunk offset {}",
                        source.kind, source.id, chunk.start_char
                    )
                })?;
        }
        Ok(())
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

    /// Atomically publishes a fully indexed staging generation.
    pub async fn replace_session_from_staging(
        &self,
        session_id: &str,
        staging_session_id: &str,
    ) -> Result<usize> {
        let store = self.store.lock().await;
        store.replace_session_from_staging(&self.scope, session_id, staging_session_id)
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
}

fn artifact_index_text(artifact: &ContextArtifact, body: &str) -> String {
    let mut text = format!(
        "Attached context: {}\nKind: {}\nArtifact ID: {}",
        artifact.title, artifact.kind, artifact.id
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
    text
}

fn artifact_context_text(artifact: &ContextArtifact) -> Result<Option<String>> {
    let mut markdown_error_kind = None;
    if let Some(path) = artifact
        .markdown_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        match read_text_prefix(Path::new(path), MAX_CONTEXT_MARKDOWN_INDEX_CHARS) {
            Ok(markdown) if !markdown.trim().is_empty() => return Ok(Some(markdown)),
            Ok(_) => {}
            Err(error) => markdown_error_kind = Some(error.kind()),
        }
    }

    let preview = artifact
        .text_preview
        .as_deref()
        .filter(|preview| !preview.trim().is_empty())
        .map(ToOwned::to_owned);
    if preview.is_some() {
        return Ok(preview);
    }
    if let Some(kind) = markdown_error_kind {
        warn!(
            artifact_id = %artifact.id,
            artifact_kind = %artifact.kind,
            error_kind = ?kind,
            "skipping unavailable local artifact text during RAG rebuild"
        );
    }
    Ok(None)
}

fn read_text_prefix(path: &Path, max_chars: usize) -> std::io::Result<String> {
    let text = fs::read_to_string(path)?;
    let mut chars = text.chars();
    let mut prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        prefix.push_str("\n\n...");
    }
    Ok(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::ContextKind;

    #[test]
    fn context_index_header_never_contains_absolute_local_path() {
        let artifact = ContextArtifact::new(
            ContextKind::Document,
            "/Users/private-name/Documents/resume.pdf",
            "Resume",
            None,
            Some(123),
        )
        .with_text_preview("Public profile experience");
        let indexed = artifact_index_text(&artifact, "Public profile experience");

        assert!(!indexed.contains("/Users/private-name"));
        assert!(indexed.contains(&artifact.id.to_string()));
        assert!(indexed.contains("Resume"));
    }

    #[test]
    fn missing_markdown_without_preview_is_skipped_not_failed() {
        let artifact = ContextArtifact::new(
            ContextKind::Document,
            "/old/device/resume.pdf",
            "Resume",
            None,
            Some(123),
        )
        .with_markdown_path(format!(
            "/tmp/bluey-missing-rag-markdown-{}.md",
            uuid::Uuid::new_v4()
        ))
        .with_processing_status(ContextProcessingStatus::Ready);

        assert_eq!(artifact_context_text(&artifact).unwrap(), None);
    }
}
