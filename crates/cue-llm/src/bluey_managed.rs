//! BlueyManagedProvider: implements `LlmProvider` by dispatching through
//! `bluey-server` via `cue-cloud-client`.
//!
//! When a customer is logged in (the daemon has tokens in keyring), this
//! provider replaces the direct OpenAI/Anthropic providers in the cue-llm
//! stack. Bluey holds the upstream API keys; the customer pays Bluey.

use async_trait::async_trait;
use cue_cloud_client::{CloudClient, CompleteRequest as CloudCompleteRequest, Error as CloudError};

use crate::{LlmChunk, LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};

/// Codex Stage 9a (S5 round-2 nit): make managed local lane
/// unrepresentable. The managed cloud has no local models — Stage 4 S4.4
/// returns 400 for `lane=local` server-side. Constructing a
/// `BlueyManagedProvider` for a local lane is now a type error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedLane {
    Instant,
    Balanced,
    Deep,
    Vision,
}

impl ManagedLane {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Instant => "instant",
            Self::Balanced => "balanced",
            Self::Deep => "deep",
            Self::Vision => "vision",
        }
    }

    fn provider_name(&self) -> &'static str {
        match self {
            Self::Instant => "bluey-managed-instant",
            Self::Balanced => "bluey-managed-balanced",
            Self::Deep => "bluey-managed-deep",
            Self::Vision => "bluey-managed-vision",
        }
    }
}

pub struct BlueyManagedProvider {
    client: CloudClient,
    lane: ManagedLane,
    name: &'static str,
}

impl BlueyManagedProvider {
    pub fn new(client: CloudClient, lane: ManagedLane) -> Self {
        let name = lane.provider_name();
        Self { client, lane, name }
    }
}

#[async_trait]
impl LlmProvider for BlueyManagedProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        // Codex Stage 9b: prefer the caller-supplied stable request_id
        // (cue-router mints it once per logical request). Fall back to
        // a per-call UUID for older code paths that haven\'t threaded
        // through yet.
        let request_id = req
            .request_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let cloud_req = CloudCompleteRequest {
            request_id,
            system: req.system.clone(),
            user: req.user.clone(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            lane: self.lane.as_str().to_string(),
            estimated_input_tokens: None,
        };
        let resp = self
            .client
            .auth_post::<_, cue_cloud_client::CompleteResponse>("/router/complete", &cloud_req)
            .await
            .map_err(map_err)?;
        Ok(LlmResponse { text: resp.text })
    }

    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        // Wrap complete() in a single-chunk stream until server-side
        // streaming proxy lands.
        let resp = self.complete(req).await?;
        let chunk = Ok(LlmChunk {
            text: resp.text,
            finished: true,
        });
        Ok(Box::pin(futures_util::stream::once(async move { chunk })))
    }
}

fn map_err(e: CloudError) -> LlmError {
    match e {
        // Codex S5.2: managed billing failures terminal (no failover).
        CloudError::Unauthorized => {
            LlmError::Billing("bluey account login required (run `bluey login`)".into())
        }
        CloudError::TrialEnded => {
            LlmError::Billing("free trial complete; reload your account to continue".into())
        }
        CloudError::InsufficientBalance {
            balance_cents,
            needed_cents,
            reload_url,
        } => LlmError::Billing(format!(
            "balance ${:.2} insufficient (need ${:.2}); reload at {}",
            balance_cents as f64 / 100.0,
            needed_cents as f64 / 100.0,
            reload_url,
        )),
        CloudError::RateLimited { retry_after_secs } => {
            LlmError::Provider(format!("rate limited; retry in {retry_after_secs}s"))
        }
        CloudError::Server { status } => LlmError::Provider(format!("server error: {status}")),
        other => LlmError::Provider(other.to_string()),
    }
}
