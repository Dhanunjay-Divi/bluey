//! LLM provider abstraction with failover router.

pub mod anthropic;
pub mod bluey_managed;
pub mod ollama;
pub mod openai;
mod router;

pub use router::LlmRouter;

use async_trait::async_trait;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use thiserror::Error;

#[derive(Debug, Clone, Default)]
pub struct LlmRequest {
    pub system: String,
    pub user: String,
    /// Optional Bluey session id for managed/cloud paths that can retrieve
    /// account-scoped session memory. Direct BYOK/local providers ignore it.
    pub session_id: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    /// Provider-neutral reasoning effort requested by the caller. Managed
    /// Bluey routes map this to safe provider-specific thinking controls.
    pub reasoning_effort: Option<String>,
    /// Optional thinking token budget for provider families that expose one.
    pub thinking_budget_tokens: Option<u32>,
    /// Codex Stage 9b: stable logical request id. When present, the
    /// managed provider passes it through to bluey-server's idempotency
    /// layer so retries (network timeout, daemon-side retry) hit the
    /// cached response instead of double-charging. `None` means the
    /// caller doesn't care about cross-call dedupe; the managed
    /// provider then mints a per-call UUID for safety inside one call's
    /// token-refresh retry window.
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<LlmCostMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<LlmArtifactMetadata>,
}

/// A single chunk from a streaming LLM completion.
#[derive(Debug, Clone)]
pub struct LlmChunk {
    pub text: String,
    pub finished: bool,
    pub cost: Option<LlmCostMetadata>,
    pub cost_label: Option<String>,
    pub artifact: Option<LlmArtifactMetadata>,
}

/// Billing/provider metadata returned by managed Bluey requests.
///
/// Direct BYOK/local providers leave this as `None`; managed providers fill it
/// so the daemon, overlay, and dashboard can show per-answer cost and the
/// post-request balance without re-querying the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmCostMetadata {
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: Option<i64>,
    pub trial_seconds_remaining: Option<i64>,
}

/// Structured artifact metadata returned by managed Bluey.
///
/// The server is the source of truth for these fields once requests route
/// through bluey-server. Local/direct providers can leave this as `None`; the
/// overlay may still infer a canvas shape as a fallback for non-managed paths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmArtifactMetadata {
    pub artifact_type: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
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
    /// Terminal billing failure from a managed provider. Codex Stage 5
    /// S5.2: managed-cloud auth/quota/balance failures must NOT trigger
    /// failover to a direct OpenAI/Anthropic provider that the daemon
    /// might also have registered, because that would be unmetered use.
    /// Any error of this variant short-circuits the failover loop and
    /// surfaces directly to the caller.
    #[error("billing failure: {0}")]
    Billing(String),
}

impl LlmError {
    pub fn should_failover(&self) -> bool {
        matches!(self, Self::Auth | Self::Quota(_))
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_))
    }
}

/// Type alias for a boxed stream of LLM chunks.
pub type LlmChunkStream = Pin<Box<dyn Stream<Item = Result<LlmChunk, LlmError>> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;

    /// Streaming completion. Default impl falls back to `complete()` and yields
    /// a single chunk — providers that natively stream override this.
    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        let resp = self.complete(req).await?;
        Ok(Box::pin(futures_util::stream::once(async move {
            Ok(LlmChunk {
                text: resp.text,
                finished: true,
                cost: resp.cost,
                cost_label: resp.cost_label,
                artifact: resp.artifact,
            })
        })))
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}
