//! BlueyManagedProvider: implements `LlmProvider` by dispatching through
//! `bluey-server` via `cue-cloud-client`.
//!
//! When a customer is logged in (the daemon has tokens in keyring), this
//! provider replaces the direct OpenAI/Anthropic providers in the cue-llm
//! stack. Bluey holds the upstream API keys; the customer pays Bluey.

use async_trait::async_trait;
use cue_cloud_client::{
    CloudClient, CompleteRequest as CloudCompleteRequest,
    CompleteResponse as CloudCompleteResponse, Error as CloudError,
};
use futures_util::StreamExt;

use crate::{
    LlmArtifactMetadata, LlmChunk, LlmChunkStream, LlmCostMetadata, LlmError, LlmProvider,
    LlmRequest, LlmResponse,
};

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
        true
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
            session_id: req.session_id.clone(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            reasoning_effort: req.reasoning_effort.clone(),
            thinking_budget_tokens: req.thinking_budget_tokens,
            lane: self.lane.as_str().to_string(),
            estimated_input_tokens: None,
        };
        let resp = self
            .client
            .auth_post::<_, cue_cloud_client::CompleteResponse>("/router/complete", &cloud_req)
            .await
            .map_err(map_err)?;
        let cost = cost_from_response(&resp);
        let artifact = artifact_from_response(&resp);
        Ok(LlmResponse {
            text: resp.text,
            cost: Some(cost),
            cost_label: resp.cost_label,
            artifact,
        })
    }

    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        let request_id = req
            .request_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let cloud_req = CloudCompleteRequest {
            request_id,
            system: req.system.clone(),
            user: req.user.clone(),
            session_id: req.session_id.clone(),
            max_tokens: req.max_tokens,
            temperature: req.temperature,
            reasoning_effort: req.reasoning_effort.clone(),
            thinking_budget_tokens: req.thinking_budget_tokens,
            lane: self.lane.as_str().to_string(),
            estimated_input_tokens: None,
        };
        let response = self
            .client
            .auth_post_stream("/router/complete/stream", &cloud_req)
            .await
            .map_err(map_err)?;
        let mut bytes = response.bytes_stream();
        let stream = async_stream::try_stream! {
            let mut buffer = String::new();
            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|e| LlmError::Network(e.to_string()))?;
                let part = std::str::from_utf8(&chunk)
                    .map_err(|e| LlmError::Provider(format!("invalid managed SSE utf8: {e}")))?;
                buffer.push_str(part);
                for parsed in parse_managed_sse_chunks(&mut buffer) {
                    yield parsed?;
                }
            }
            for parsed in drain_managed_sse_tail(&mut buffer) {
                yield parsed?;
            }
        };
        Ok(Box::pin(stream))
    }
}

fn parse_managed_sse_chunks(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let mut chunks = Vec::new();
    while let Some(frame) = take_sse_frame(buffer) {
        parse_managed_sse_frame(&frame, &mut chunks);
    }
    chunks
}

fn drain_managed_sse_tail(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let tail = std::mem::take(buffer);
    if tail.trim().is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    parse_managed_sse_frame(&tail, &mut chunks);
    chunks
}

fn take_sse_frame(buffer: &mut String) -> Option<String> {
    let lf = buffer.find("\n\n").map(|pos| (pos, 2));
    let crlf = buffer.find("\r\n\r\n").map(|pos| (pos, 4));
    let (pos, sep_len) = match (lf, crlf) {
        (Some(a), Some(b)) => {
            if a.0 <= b.0 {
                a
            } else {
                b
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    let frame = buffer[..pos].to_string();
    *buffer = buffer[pos + sep_len..].to_string();
    Some(frame)
}

fn parse_managed_sse_frame(frame: &str, chunks: &mut Vec<Result<LlmChunk, LlmError>>) {
    let mut event = "message";
    let mut data_lines = Vec::new();
    for line in frame.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim();
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim_start().to_string());
        }
    }
    if data_lines.is_empty() {
        return;
    }
    let data = data_lines.join("\n");
    if data == "[DONE]" {
        chunks.push(Ok(LlmChunk {
            text: String::new(),
            finished: true,
            cost: None,
            cost_label: None,
            artifact: None,
        }));
        return;
    }
    if event == "billing" {
        match serde_json::from_str::<CloudCompleteResponse>(&data) {
            Ok(resp) => {
                let artifact = artifact_from_response(&resp);
                let cost_label = resp.cost_label.clone();
                chunks.push(Ok(LlmChunk {
                    text: String::new(),
                    finished: true,
                    cost: Some(cost_from_response(&resp)),
                    cost_label,
                    artifact,
                }));
            }
            Err(e) => chunks.push(Err(LlmError::Provider(format!(
                "managed billing SSE parse error: {e}"
            )))),
        }
        return;
    }
    match serde_json::from_str::<serde_json::Value>(&data) {
        Ok(parsed) => {
            if let Some(delta) = parsed
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"))
                .and_then(|d| d.get("content"))
                .and_then(|c| c.as_str())
            {
                chunks.push(Ok(LlmChunk {
                    text: delta.to_string(),
                    finished: false,
                    cost: None,
                    cost_label: None,
                    artifact: None,
                }));
            }
        }
        Err(e) => chunks.push(Err(LlmError::Provider(format!(
            "managed SSE parse error: {e}"
        )))),
    }
}

fn cost_from_response(resp: &CloudCompleteResponse) -> LlmCostMetadata {
    LlmCostMetadata {
        provider: resp.provider.clone(),
        model: resp.model.clone(),
        input_tokens: resp.input_tokens,
        output_tokens: resp.output_tokens,
        cost_cents: resp.cost_cents,
        balance_cents_after: Some(resp.balance_cents_after),
        trial_seconds_remaining: Some(resp.trial_seconds_remaining),
    }
}

fn artifact_from_response(resp: &CloudCompleteResponse) -> Option<LlmArtifactMetadata> {
    Some(LlmArtifactMetadata {
        artifact_type: resp.artifact_type.clone()?,
        body: resp.artifact_body.clone()?,
        confidence: resp.confidence,
    })
}

fn map_err(e: CloudError) -> LlmError {
    match e {
        // Codex S5.2: managed billing failures terminal (no failover).
        CloudError::Unauthorized => {
            LlmError::Billing("Bluey sign-in required; run `bluey on` to finish setup".into())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_managed_sse_deltas_and_billing_metadata() {
        let mut buffer = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
            "event: billing\n",
            "data: {\"text\":\"Hello world\",\"provider\":\"openai\",\"model\":\"gpt-4o-mini\",\"input_tokens\":12,\"output_tokens\":7,\"cost_cents\":3,\"balance_cents_after\":2997,\"trial_seconds_remaining\":0,\"cost_label\":\"$0.03 · balance $29.97\",\"artifact_type\":\"code\",\"artifact_body\":\"CODE\\n----\\nfn main() {}\",\"confidence\":0.95}\n\n",
            "data: [DONE]\n\n",
        )
        .to_string();

        let chunks = parse_managed_sse_chunks(&mut buffer)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(chunks[0].text, "Hello ");
        assert!(!chunks[0].finished);
        assert_eq!(chunks[1].text, "world");
        assert!(!chunks[1].finished);
        assert!(chunks[2].finished);
        let cost = chunks[2].cost.as_ref().unwrap();
        assert_eq!(cost.cost_cents, 3);
        assert_eq!(cost.balance_cents_after, Some(2997));
        assert_eq!(cost.provider, "openai");
        assert_eq!(cost.model, "gpt-4o-mini");
        assert_eq!(
            chunks[2].cost_label.as_deref(),
            Some("$0.03 · balance $29.97")
        );
        let artifact = chunks[2].artifact.as_ref().expect("artifact metadata");
        assert_eq!(artifact.artifact_type, "code");
        assert!(artifact.body.contains("fn main"));
        assert_eq!(artifact.confidence, Some(0.95));
    }

    #[test]
    fn parses_crlf_sse_frames() {
        let mut buffer =
            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\r\n\r\n".to_string();
        let chunks = parse_managed_sse_chunks(&mut buffer)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "ok");
        assert!(buffer.is_empty());
    }
}
