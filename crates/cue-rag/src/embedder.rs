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
    /// Stable model identity used to prevent cross-model vector searches.
    ///
    /// Existing providers default to their provider name for compatibility;
    /// providers that can change models should override this explicitly.
    fn model(&self) -> &'static str {
        self.name()
    }
    fn dim(&self) -> usize;
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let mut vectors = Vec::with_capacity(texts.len());
        for text in texts {
            vectors.push(self.embed(text).await?);
        }
        Ok(vectors)
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

    fn model(&self) -> &'static str {
        Self::MODEL
    }

    fn dim(&self) -> usize {
        Self::DIM
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let mut vectors = self.embed_batch(&[text.to_string()]).await?;
        vectors
            .pop()
            .ok_or_else(|| EmbeddingError::InvalidResponse("missing embedding".into()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = serde_json::json!({
            "model": Self::MODEL,
            "input": texts,
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

        let data = json["data"]
            .as_array()
            .ok_or_else(|| EmbeddingError::InvalidResponse("missing data".into()))?;
        if data.len() != texts.len() {
            return Err(EmbeddingError::InvalidResponse(format!(
                "expected {} embeddings, got {}",
                texts.len(),
                data.len()
            )));
        }

        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let embedding = item["embedding"]
                .as_array()
                .ok_or_else(|| EmbeddingError::InvalidResponse("missing embedding".into()))?
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
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }
}

// Future providers (Gemini, Ollama, local ONNX) will implement EmbeddingProvider.
// The trait shape supports them without changes.
