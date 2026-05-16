//! LLM provider abstraction with failover router.

pub mod anthropic;
pub mod ollama;
pub mod openai;
mod router;

pub use router::LlmRouter;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub system: String,
    pub user: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("authentication failed")]
    Auth,
    #[error("quota exceeded: {0}")]
    Quota(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("provider error: {0}")]
    Provider(String),
}

impl LlmError {
    pub fn should_failover(&self) -> bool {
        matches!(self, Self::Auth | Self::Quota(_))
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_))
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;
    fn supports_streaming(&self) -> bool {
        false
    }
}
