//! Embedding provider trait and OpenAI implementation.
//!
//! The trait is designed for easy extension to Gemini, Ollama, or local
//! models in follow-up rounds.

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("API request failed: {0}")]
    Request(String),
    #[error("no API key configured")]
    NoApiKey,
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

/// Pluggable embedding provider. Implementations must be Send + Sync for
/// use across async tasks.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn dim(&self) -> usize;
    /// Embed a PASSAGE / document (what gets indexed).
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    /// Embed a retrieval QUERY. Local models apply a model-specific query prompt
    /// (bge/arctic prefix, Gemma `task:… | query:`); providers with no query/doc
    /// asymmetry (e.g. OpenAI) default to plain [`Self::embed`].
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        self.embed(text).await
    }

    /// The retrieval score floor calibrated for THIS model's cosine
    /// distribution. Measured live per model — bge/arctic sit ~0.45, Gemma's
    /// scores are compressed to ~0.20 (proven in `tests/embedder_bakeoff.rs`).
    /// The default suits the bge family.
    fn relevance_floor(&self) -> f32 {
        0.45
    }

    /// Blocking PASSAGE embed for callers that build an index from a synchronous
    /// closure (e.g. the agent-history index rebuild, already off the async
    /// runtime). Local ONNX models override this to run inference directly
    /// without a runtime hop; the default bridges to the async [`Self::embed`]
    /// via a transient current-thread runtime, so remote providers still work.
    fn embed_passage_blocking(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| EmbeddingError::Request(format!("blocking rt: {e}")))?
            .block_on(self.embed(text))
    }
}

/// OpenAI text-embedding-3-small (1536 dimensions).
pub struct OpenAiEmbedder {
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiEmbedder {
    pub const DIM: usize = 1536;
    pub const MODEL: &'static str = "text-embedding-3-small";

    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbedder {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn dim(&self) -> usize {
        Self::DIM
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let body = serde_json::json!({
            "model": Self::MODEL,
            "input": text,
        });
        let resp = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| EmbeddingError::Request(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(EmbeddingError::Request(format!("{status}: {text}")));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EmbeddingError::InvalidResponse(e.to_string()))?;

        let embedding = json["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| EmbeddingError::InvalidResponse("missing data[0].embedding".into()))?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect::<Vec<f32>>();

        if embedding.len() != Self::DIM {
            return Err(EmbeddingError::InvalidResponse(format!(
                "expected {} dims, got {}",
                Self::DIM,
                embedding.len()
            )));
        }

        Ok(embedding)
    }
}

// Future providers (Gemini, Ollama, local ONNX) will implement EmbeddingProvider.
// The trait shape supports them without changes.
