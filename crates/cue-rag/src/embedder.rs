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
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;
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
