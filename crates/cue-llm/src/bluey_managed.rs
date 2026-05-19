//! BlueyManagedProvider: implements `LlmProvider` by dispatching through
//! `bluey-server` via `cue-cloud-client`.
//!
//! When a customer is logged in (the daemon has tokens in keyring), this
//! provider replaces the direct OpenAI/Anthropic providers in the cue-llm
//! stack. Bluey holds the upstream API keys; the customer pays Bluey.

use async_trait::async_trait;
use cue_cloud_client::{CloudClient, CompleteRequest as CloudCompleteRequest, Error as CloudError};

use crate::{LlmChunk, LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};

/// LLM provider that routes through bluey-server.
///
/// Each instance is bound to a specific `lane` (instant / balanced /
/// deep / vision / local). Higher-level routing decides which lane to
/// pick; this provider is "the dispatcher for one lane".
pub struct BlueyManagedProvider {
    client: CloudClient,
    lane: String,
    name: &'static str,
}

impl BlueyManagedProvider {
    pub fn new(client: CloudClient, lane: impl Into<String>) -> Self {
        let lane = lane.into();
        let name = match lane.as_str() {
            "instant" => "bluey-managed-instant",
            "balanced" => "bluey-managed-balanced",
            "deep" => "bluey-managed-deep",
            "vision" => "bluey-managed-vision",
            "local" => "bluey-managed-local",
            _ => "bluey-managed",
        };
        Self { client, lane, name }
    }
}

#[async_trait]
impl LlmProvider for BlueyManagedProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn supports_streaming(&self) -> bool {
        // v0.2 server returns full text in one shot; we adapt by emitting
        // a single chunk in complete_stream(). Real streaming through
        // bluey-server lands in v0.2.x.
        false
    }

    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let cloud_req = CloudCompleteRequest {
            system: req.system.clone(),
            user: req.user.clone(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            lane: self.lane.clone(),
            estimated_input_tokens: None,
        };
        let resp = self
            .client
            .auth_post::<_, cue_cloud_client::CompleteResponse>("/router/complete", &cloud_req)
            .await
            .map_err(map_err)?;
        Ok(LlmResponse { text: resp.text })
    }

    async fn complete_stream(
        &self,
        req: &LlmRequest,
    ) -> Result<LlmChunkStream, LlmError> {
        // Wrap complete() in a single-chunk stream until server-side
        // streaming proxy lands.
        let resp = self.complete(req).await?;
        let chunk = Ok(LlmChunk { text: resp.text, finished: true });
        Ok(Box::pin(futures_util::stream::once(async move { chunk })))
    }
}

fn map_err(e: CloudError) -> LlmError {
    match e {
        CloudError::Unauthorized => LlmError::Auth,
        CloudError::TrialEnded => LlmError::Quota(
            "free trial complete; load $30 to continue at https://bluey.dev/reload".into(),
        ),
        CloudError::InsufficientBalance {
            balance_cents,
            needed_cents,
            ..
        } => LlmError::Quota(format!(
            "balance ${:.2} insufficient (need ${:.2}); reload at https://bluey.dev/reload",
            balance_cents as f64 / 100.0,
            needed_cents as f64 / 100.0,
        )),
        CloudError::RateLimited { retry_after_secs } => {
            LlmError::Provider(format!("rate limited; retry in {retry_after_secs}s"))
        }
        CloudError::Server { status, body } => {
            LlmError::Provider(format!("server {status}: {body}"))
        }
        other => LlmError::Provider(other.to_string()),
    }
}
