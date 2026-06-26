//! BlueyManagedProvider: implements `LlmProvider` by dispatching through
//! `bluey-server` via `cue-cloud-client`.
//!
//! When a customer is logged in (the daemon has Bluey account tokens), this
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
    LlmRequest, LlmResponse, LlmSourceMetadata, LlmStatusMetadata,
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
            image_data_urls: req.image_data_urls.clone(),
        };
        let resp = self
            .client
            .auth_post::<_, cue_cloud_client::CompleteResponse>("/router/complete", &cloud_req)
            .await
            .map_err(map_err)?;
        let cost = cost_from_response(&resp);
        let artifact = artifact_from_response(&resp);
        let sources = sources_from_response(&resp);
        Ok(LlmResponse {
            text: resp.text,
            cost: Some(cost),
            cost_label: resp.cost_label.clone(),
            artifact,
            sources,
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
            image_data_urls: req.image_data_urls.clone(),
        };
        let response = self
            .client
            .auth_post_stream("/router/complete/stream", &cloud_req)
            .await
            .map_err(map_err)?;
        let mut bytes = response.bytes_stream();
        let stream = async_stream::try_stream! {
            let mut buffer = String::new();
            let mut pending_utf8 = Vec::new();
            let mut seen_billing_final = false;
            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|e| LlmError::Network(e.to_string()))?;
                append_utf8_chunk(&chunk, &mut pending_utf8, &mut buffer)?;
                for parsed in parse_managed_stream_chunks(&mut buffer) {
                    let parsed = parsed?;
                    validate_managed_stream_finality(&parsed, &mut seen_billing_final)?;
                    yield parsed;
                }
            }
            if !pending_utf8.is_empty() {
                let tail = std::str::from_utf8(&pending_utf8)
                    .map_err(|e| LlmError::Provider(format!("incomplete managed stream utf8: {e}")))?;
                buffer.push_str(tail);
            }
            for parsed in drain_managed_stream_tail(&mut buffer) {
                let parsed = parsed?;
                validate_managed_stream_finality(&parsed, &mut seen_billing_final)?;
                yield parsed;
            }
            if !seen_billing_final {
                Err::<(), LlmError>(LlmError::Provider(
                    "managed stream ended before final billing metadata".to_string(),
                ))?;
            }
        };
        Ok(Box::pin(stream))
    }
}

fn validate_managed_stream_finality(
    chunk: &LlmChunk,
    seen_billing_final: &mut bool,
) -> Result<(), LlmError> {
    if is_managed_billing_final(chunk) {
        *seen_billing_final = true;
        return Ok(());
    }
    if chunk.finished && !*seen_billing_final {
        return Err(LlmError::Provider(
            "managed stream ended before final billing metadata".to_string(),
        ));
    }
    Ok(())
}

fn append_utf8_chunk(
    bytes: &[u8],
    pending_utf8: &mut Vec<u8>,
    buffer: &mut String,
) -> Result<(), LlmError> {
    pending_utf8.extend_from_slice(bytes);
    loop {
        match std::str::from_utf8(pending_utf8) {
            Ok(valid) => {
                buffer.push_str(valid);
                pending_utf8.clear();
                return Ok(());
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&pending_utf8[..valid_up_to])
                        .expect("valid_up_to always marks utf8");
                    buffer.push_str(valid);
                    pending_utf8.drain(..valid_up_to);
                    continue;
                }
                if err.error_len().is_some() {
                    return Err(LlmError::Provider(format!(
                        "invalid managed stream utf8: {err}"
                    )));
                }
                return Ok(());
            }
        }
    }
}

fn parse_managed_stream_chunks(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let mut chunks = Vec::new();
    loop {
        if let Some(frame) = take_sse_frame(buffer) {
            parse_managed_sse_frame(&frame, &mut chunks);
            continue;
        }
        if let Some(record) = take_ndjson_record(buffer) {
            parse_managed_ndjson_record(&record, &mut chunks);
            continue;
        }
        break;
    }
    chunks
}

#[cfg(test)]
fn parse_managed_sse_chunks(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    parse_managed_stream_chunks(buffer)
}

fn drain_managed_stream_tail(buffer: &mut String) -> Vec<Result<LlmChunk, LlmError>> {
    let tail = std::mem::take(buffer);
    if tail.trim().is_empty() {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let tail = tail.trim();
    if looks_like_json_record(tail) || tail == "[DONE]" {
        parse_managed_ndjson_record(tail, &mut chunks);
    } else {
        parse_managed_sse_frame(tail, &mut chunks);
    }
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

fn take_ndjson_record(buffer: &mut String) -> Option<String> {
    let (start, first) = buffer.char_indices().find(|(_, c)| !c.is_whitespace())?;
    if !matches!(first, '{' | '[') {
        return None;
    }
    let line_end = buffer[start..].find('\n')?;
    let end = start + line_end;
    let record = buffer[start..end].trim_end_matches('\r').to_string();
    *buffer = buffer[end + 1..].to_string();
    Some(record)
}

fn looks_like_json_record(value: &str) -> bool {
    matches!(value.chars().next(), Some('{') | Some('['))
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
    parse_managed_payload(event, &data, chunks);
}

fn parse_managed_ndjson_record(record: &str, chunks: &mut Vec<Result<LlmChunk, LlmError>>) {
    parse_managed_payload("message", record, chunks);
}

fn parse_managed_payload(event: &str, data: &str, chunks: &mut Vec<Result<LlmChunk, LlmError>>) {
    let data = data.trim();
    if data.is_empty() {
        return;
    }
    if data == "[DONE]" {
        chunks.push(Ok(done_chunk()));
        return;
    }
    match serde_json::from_str::<serde_json::Value>(data) {
        Ok(parsed) => parse_managed_json_event(event, &parsed, chunks),
        Err(_) if is_plain_text_event(event) => {
            chunks.push(Ok(text_chunk(data.to_string(), false)));
        }
        Err(e) => chunks.push(Err(LlmError::Provider(format!(
            "managed stream parse error: {e}"
        )))),
    }
}

fn parse_managed_json_event(
    event: &str,
    parsed: &serde_json::Value,
    chunks: &mut Vec<Result<LlmChunk, LlmError>>,
) {
    if is_error_event(event, parsed) {
        chunks.push(Err(LlmError::Provider(error_message_from_value(parsed))));
        return;
    }

    let finalish = is_final_event(event, parsed);
    if finalish {
        chunks.push(Ok(final_chunk_from_value(event, parsed)));
        return;
    }

    if let Some(status) = status_metadata_from_value(event, parsed) {
        chunks.push(Ok(status_chunk(status)));
        return;
    }

    let sources = sources_from_value(parsed);
    if !sources.is_empty() {
        chunks.push(Ok(sources_chunk(sources)));
        return;
    }

    if let Some(delta) = extract_delta_text(parsed) {
        chunks.push(Ok(text_chunk(delta, false)));
        return;
    }

    if json_bool(parsed, &["finished", "done", "final"]) {
        chunks.push(Ok(done_chunk()));
    }
}

fn is_plain_text_event(event: &str) -> bool {
    matches!(
        event.trim().to_ascii_lowercase().as_str(),
        "chunk" | "delta" | "text"
    )
}

fn is_error_event(event: &str, value: &serde_json::Value) -> bool {
    event.eq_ignore_ascii_case("error")
        || stream_kind(value).as_deref() == Some("error")
        || value.get("error").is_some()
}

fn is_final_event(event: &str, value: &serde_json::Value) -> bool {
    let event = event.trim().to_ascii_lowercase();
    matches!(
        event.as_str(),
        "billing" | "complete" | "completion" | "done" | "final" | "metadata"
    ) || matches!(
        stream_kind(value).as_deref(),
        Some("billing" | "complete" | "completion" | "done" | "final" | "metadata")
    ) || json_bool(value, &["finished", "done", "final"])
        || looks_like_final_metadata(value)
}

fn stream_kind(value: &serde_json::Value) -> Option<String> {
    for key in ["type", "event", "kind"] {
        if let Some(kind) = value.get(key).and_then(|v| v.as_str()) {
            return Some(kind.to_ascii_lowercase());
        }
    }
    None
}

fn looks_like_final_metadata(value: &serde_json::Value) -> bool {
    let has_billing_or_artifact = has_any_metadata_key(
        value,
        &[
            "cost_cents",
            "balance_cents_after",
            "trial_seconds_remaining",
            "cost_label",
            "artifact_type",
            "artifact_body",
        ],
    );
    let has_provider_usage =
        cost_metadata_from_value(value).is_some() && has_any_usage_metadata(value);
    has_billing_or_artifact || has_provider_usage
}

fn final_chunk_from_value(event: &str, value: &serde_json::Value) -> LlmChunk {
    if let Ok(resp) = serde_json::from_value::<CloudCompleteResponse>(value.clone()) {
        let cost_label = resp.cost_label.clone();
        return LlmChunk {
            text: String::new(),
            finished: true,
            cost: Some(cost_from_response(&resp)),
            cost_label,
            artifact: artifact_from_response(&resp),
            status: None,
            sources: sources_from_response(&resp),
        };
    }

    let metadata_only = event.eq_ignore_ascii_case("billing")
        || event.eq_ignore_ascii_case("metadata")
        || looks_like_final_metadata(value);
    LlmChunk {
        text: if metadata_only {
            String::new()
        } else {
            extract_delta_text(value).unwrap_or_default()
        },
        finished: true,
        cost: cost_metadata_from_value(value),
        cost_label: find_string_in_sources(value, &["cost_label", "label"]),
        artifact: artifact_from_value(value),
        status: None,
        sources: sources_from_value(value),
    }
}

fn done_chunk() -> LlmChunk {
    LlmChunk {
        text: String::new(),
        finished: true,
        cost: None,
        cost_label: None,
        artifact: None,
        status: None,
        sources: Vec::new(),
    }
}

fn text_chunk(text: String, finished: bool) -> LlmChunk {
    LlmChunk {
        text,
        finished,
        cost: None,
        cost_label: None,
        artifact: None,
        status: None,
        sources: Vec::new(),
    }
}

fn status_chunk(status: LlmStatusMetadata) -> LlmChunk {
    LlmChunk {
        text: String::new(),
        finished: false,
        cost: None,
        cost_label: None,
        artifact: None,
        status: Some(status),
        sources: Vec::new(),
    }
}

fn sources_chunk(sources: Vec<LlmSourceMetadata>) -> LlmChunk {
    LlmChunk {
        text: String::new(),
        finished: false,
        cost: None,
        cost_label: None,
        artifact: None,
        status: None,
        sources,
    }
}

fn is_managed_billing_final(chunk: &LlmChunk) -> bool {
    chunk.finished && chunk.cost.is_some()
}

fn extract_delta_text(value: &serde_json::Value) -> Option<String> {
    value
        .pointer("/choices/0/delta/content")
        .and_then(|v| v.as_str())
        .or_else(|| value.pointer("/delta/text").and_then(|v| v.as_str()))
        .or_else(|| value.get("delta").and_then(|v| v.as_str()))
        .or_else(|| value.get("text").and_then(|v| v.as_str()))
        .or_else(|| value.get("content").and_then(|v| v.as_str()))
        .or_else(|| value.get("chunk").and_then(|v| v.as_str()))
        .map(ToOwned::to_owned)
}

fn cost_metadata_from_value(value: &serde_json::Value) -> Option<LlmCostMetadata> {
    let provider = find_string_in_sources(value, &["provider"])?;
    let model = find_string_in_sources(value, &["model"])?;
    Some(LlmCostMetadata {
        provider,
        model,
        input_tokens: find_i64_in_sources(value, &["input_tokens", "prompt_tokens"]).unwrap_or(0),
        output_tokens: find_i64_in_sources(value, &["output_tokens", "completion_tokens"])
            .unwrap_or(0),
        cost_cents: find_i64_in_sources(value, &["cost_cents"]).unwrap_or(0),
        balance_cents_after: find_i64_in_sources(value, &["balance_cents_after"]),
        trial_seconds_remaining: find_i64_in_sources(value, &["trial_seconds_remaining"]),
    })
}

fn artifact_from_value(value: &serde_json::Value) -> Option<LlmArtifactMetadata> {
    if let (Some(artifact_type), Some(body)) = (
        find_string_in_sources(value, &["artifact_type"]),
        find_string_in_sources(value, &["artifact_body"]),
    ) {
        return Some(LlmArtifactMetadata {
            artifact_type,
            body,
            confidence: find_f32_in_sources(value, &["confidence", "artifact_confidence"]),
        });
    }

    for source in metadata_sources(value) {
        if let Some(artifact) = source.get("artifact") {
            let Some(artifact_type) = artifact.get("type").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(body) = artifact
                .get("body")
                .or_else(|| artifact.get("content"))
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            return Some(LlmArtifactMetadata {
                artifact_type: artifact_type.to_string(),
                body: body.to_string(),
                confidence: artifact
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32),
            });
        }
    }
    None
}

fn find_string_in_sources(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for source in metadata_sources(value) {
        for key in keys {
            if let Some(found) = source.get(*key).and_then(|v| v.as_str()) {
                return Some(found.to_string());
            }
        }
    }
    None
}

fn find_i64_in_sources(value: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    for source in metadata_sources(value) {
        for key in keys {
            if let Some(found) = source.get(*key).and_then(|v| v.as_i64()) {
                return Some(found);
            }
        }
        if let Some(usage) = source.get("usage") {
            for key in keys {
                if let Some(found) = usage.get(*key).and_then(|v| v.as_i64()) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn find_f32_in_sources(value: &serde_json::Value, keys: &[&str]) -> Option<f32> {
    for source in metadata_sources(value) {
        for key in keys {
            if let Some(found) = source.get(*key).and_then(|v| v.as_f64()) {
                return Some(found as f32);
            }
        }
    }
    None
}

fn has_any_metadata_key(value: &serde_json::Value, keys: &[&str]) -> bool {
    metadata_sources(value)
        .into_iter()
        .any(|source| keys.iter().any(|key| source.get(*key).is_some()))
}

fn has_any_usage_metadata(value: &serde_json::Value) -> bool {
    metadata_sources(value).into_iter().any(|source| {
        source.get("usage").is_some()
            || [
                "input_tokens",
                "output_tokens",
                "prompt_tokens",
                "completion_tokens",
            ]
            .iter()
            .any(|key| source.get(*key).is_some())
    })
}

fn metadata_sources(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut sources = vec![value];
    for key in ["metadata", "billing", "final", "response", "result"] {
        if let Some(source) = value.get(key) {
            sources.push(source);
        }
    }
    sources
}

fn json_bool(value: &serde_json::Value, keys: &[&str]) -> bool {
    keys.iter()
        .any(|key| value.get(*key).and_then(|v| v.as_bool()) == Some(true))
}

fn error_message_from_value(value: &serde_json::Value) -> String {
    value
        .get("error")
        .and_then(|error| {
            error.as_str().map(ToOwned::to_owned).or_else(|| {
                error
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned)
            })
        })
        .or_else(|| {
            value
                .get("message")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| value.to_string())
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

fn status_metadata_from_value(event: &str, value: &serde_json::Value) -> Option<LlmStatusMetadata> {
    let kind = stream_kind(value);
    let event_is_status = event.eq_ignore_ascii_case("status")
        || matches!(kind.as_deref(), Some("status" | "retrieval_status"));
    if !event_is_status {
        return None;
    }
    let stage = find_string_in_sources(value, &["stage"])
        .or(kind)
        .unwrap_or_else(|| "status".to_string());
    let message = find_string_in_sources(value, &["message", "label", "text"])
        .unwrap_or_else(|| stage.replace('_', " "));
    Some(LlmStatusMetadata { stage, message })
}

fn sources_from_response(resp: &CloudCompleteResponse) -> Vec<LlmSourceMetadata> {
    resp.sources
        .iter()
        .map(|source| LlmSourceMetadata {
            id: source.id.clone(),
            title: source.title.clone(),
            url: source.url.clone(),
            snippet: source.snippet.clone(),
            source_type: source.source_type.clone(),
        })
        .collect()
}

fn sources_from_value(value: &serde_json::Value) -> Vec<LlmSourceMetadata> {
    let candidates = value
        .get("sources")
        .or_else(|| value.get("citations"))
        .or_else(|| value.pointer("/metadata/sources"))
        .or_else(|| value.pointer("/response/sources"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    candidates
        .into_iter()
        .enumerate()
        .filter_map(|(idx, source)| {
            let title = source
                .get("title")
                .or_else(|| source.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("Source")
                .trim()
                .to_string();
            if title.is_empty() {
                return None;
            }
            let id = source
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("S{}", idx + 1));
            let url = source
                .get("url")
                .or_else(|| source.get("link"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let snippet = source
                .get("snippet")
                .or_else(|| source.get("description"))
                .or_else(|| source.get("content"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let source_type = source
                .get("source_type")
                .or_else(|| source.get("type"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Some(LlmSourceMetadata {
                id,
                title,
                url,
                snippet,
                source_type,
            })
        })
        .collect()
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
        CloudError::CapacityBusy {
            retry_after_secs,
            reason,
        } => LlmError::CapacityBusy {
            retry_after_secs,
            reason,
        },
        CloudError::Server { status } => LlmError::Provider(format!("server error: {status}")),
        other => LlmError::Provider(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LlmProvider;
    use cue_cloud_client::{client::ClientConfig, tokens::MemoryStore, CloudClient, Tokens};
    use futures_util::StreamExt;
    use std::{sync::Arc, time::Duration};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[test]
    fn parses_ndjson_deltas_and_final_metadata() {
        let mut buffer = concat!(
            "{\"type\":\"chunk\",\"text\":\"Hello \"}\n",
            "{\"delta\":{\"text\":\"world\"}}\n",
            "{\"type\":\"final\",\"metadata\":{\"provider\":\"anthropic\",\"model\":\"claude-3-5-haiku\",\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"cost_cents\":1,\"balance_cents_after\":1999,\"trial_seconds_remaining\":0,\"cost_label\":\"$0.01\"},\"artifact\":{\"type\":\"markdown\",\"body\":\"# Done\",\"confidence\":0.8}}\n",
        )
        .to_string();

        let chunks = parse_managed_stream_chunks(&mut buffer)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].text, "Hello ");
        assert_eq!(chunks[1].text, "world");
        assert!(chunks[2].finished);
        assert_eq!(chunks[2].text, "");
        let cost = chunks[2].cost.as_ref().unwrap();
        assert_eq!(cost.provider, "anthropic");
        assert_eq!(cost.model, "claude-3-5-haiku");
        assert_eq!(cost.input_tokens, 4);
        assert_eq!(cost.output_tokens, 2);
        assert_eq!(cost.cost_cents, 1);
        assert_eq!(cost.balance_cents_after, Some(1999));
        assert_eq!(chunks[2].cost_label.as_deref(), Some("$0.01"));
        let artifact = chunks[2].artifact.as_ref().unwrap();
        assert_eq!(artifact.artifact_type, "markdown");
        assert_eq!(artifact.body, "# Done");
        assert_eq!(artifact.confidence, Some(0.8));
        assert!(buffer.is_empty());
    }

    #[test]
    fn parses_status_and_source_events() {
        let mut buffer = concat!(
            "event: status\n",
            "data: {\"type\":\"status\",\"stage\":\"searching_web\",\"message\":\"Searching web\"}\n\n",
            "event: sources\n",
            "data: {\"type\":\"sources\",\"sources\":[{\"id\":\"W1\",\"title\":\"Bluey docs\",\"url\":\"https://example.com/bluey\",\"snippet\":\"A useful source\",\"source_type\":\"web\"}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Answer\"}}]}\n\n",
            "event: billing\n",
            "data: {\"text\":\"Answer\",\"provider\":\"openai\",\"model\":\"gpt-4o-mini\",\"input_tokens\":5,\"output_tokens\":1,\"cost_cents\":1,\"balance_cents_after\":999,\"trial_seconds_remaining\":0,\"sources\":[{\"id\":\"W1\",\"title\":\"Bluey docs\",\"url\":\"https://example.com/bluey\",\"snippet\":\"A useful source\",\"source_type\":\"web\"}]}\n\n",
            "data: [DONE]\n\n",
        )
        .to_string();

        let chunks = parse_managed_sse_chunks(&mut buffer)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            chunks[0]
                .status
                .as_ref()
                .map(|status| status.stage.as_str()),
            Some("searching_web")
        );
        assert_eq!(chunks[1].sources.len(), 1);
        assert_eq!(chunks[1].sources[0].id, "W1");
        assert_eq!(chunks[2].text, "Answer");
        assert!(chunks[3].finished);
        assert_eq!(chunks[3].sources.len(), 1);
    }

    #[test]
    fn buffers_split_utf8_boundaries() {
        let mut pending = Vec::new();
        let mut buffer = String::new();
        let bytes = "data: {\"text\":\"hi 👋\"}\n\n".as_bytes();
        let split_at = bytes
            .iter()
            .position(|byte| *byte == 0xF0)
            .expect("emoji first byte")
            + 1;

        append_utf8_chunk(&bytes[..split_at], &mut pending, &mut buffer).unwrap();
        assert!(buffer.ends_with("hi "));
        assert!(!pending.is_empty());
        append_utf8_chunk(&bytes[split_at..], &mut pending, &mut buffer).unwrap();

        let chunks = parse_managed_stream_chunks(&mut buffer)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(chunks[0].text, "hi 👋");
    }

    #[tokio::test]
    async fn complete_stream_posts_to_managed_endpoint_and_parses_ndjson() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .and(header("authorization", "Bearer access"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/x-ndjson")
                    .set_body_string(concat!(
                        "{\"type\":\"chunk\",\"text\":\"Hi \"}\n",
                        "{\"type\":\"chunk\",\"text\":\"there\"}\n",
                        "{\"type\":\"final\",\"metadata\":{\"provider\":\"openai\",\"model\":\"gpt-4o-mini\",\"input_tokens\":3,\"output_tokens\":2,\"cost_cents\":1,\"balance_cents_after\":499,\"trial_seconds_remaining\":0,\"cost_label\":\"$0.01\"}}\n",
                    )),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = CloudClient::new(
            ClientConfig {
                base_url: server.uri(),
                user_agent: "test".into(),
                timeout: Duration::from_secs(10),
                trace_id: None,
            },
            Arc::new(MemoryStore::new()),
        )
        .unwrap();
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let provider = BlueyManagedProvider::new(client, ManagedLane::Instant);
        let mut stream = provider
            .complete_stream(&LlmRequest {
                system: "system".into(),
                user: "user".into(),
                session_id: Some("sess-1".into()),
                max_tokens: Some(10),
                temperature: Some(0.2),
                reasoning_effort: None,
                thinking_budget_tokens: None,
                request_id: Some("req-1".into()),
                image_data_urls: Vec::new(),
            })
            .await
            .unwrap();

        let mut chunks = Vec::new();
        while let Some(chunk) = stream.next().await {
            chunks.push(chunk.unwrap());
        }

        assert_eq!(chunks[0].text, "Hi ");
        assert_eq!(chunks[1].text, "there");
        assert!(chunks[2].finished);
        assert_eq!(chunks[2].cost.as_ref().unwrap().cost_cents, 1);
        assert_eq!(chunks[2].cost_label.as_deref(), Some("$0.01"));
    }

    #[tokio::test]
    async fn complete_stream_errors_when_managed_stream_ends_without_billing_final() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .and(header("authorization", "Bearer access"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
                    ),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = CloudClient::new(
            ClientConfig {
                base_url: server.uri(),
                user_agent: "test".into(),
                timeout: Duration::from_secs(10),
                trace_id: None,
            },
            Arc::new(MemoryStore::new()),
        )
        .unwrap();
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let provider = BlueyManagedProvider::new(client, ManagedLane::Instant);
        let mut stream = provider
            .complete_stream(&LlmRequest {
                system: "system".into(),
                user: "user".into(),
                session_id: Some("sess-1".into()),
                max_tokens: Some(10),
                temperature: Some(0.2),
                reasoning_effort: None,
                thinking_budget_tokens: None,
                request_id: Some("req-truncated".into()),
                image_data_urls: Vec::new(),
            })
            .await
            .unwrap();

        let first = stream.next().await.expect("first delta").unwrap();
        assert_eq!(first.text, "partial");
        let error = stream
            .next()
            .await
            .expect("terminal stream error")
            .unwrap_err()
            .to_string();
        assert!(error.contains("final billing metadata"));
    }

    #[tokio::test]
    async fn complete_stream_errors_when_done_arrives_before_billing_final() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/router/complete/stream"))
            .and(header("authorization", "Bearer access"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
                        "data: [DONE]\n\n",
                    )),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = CloudClient::new(
            ClientConfig {
                base_url: server.uri(),
                user_agent: "test".into(),
                timeout: Duration::from_secs(10),
                trace_id: None,
            },
            Arc::new(MemoryStore::new()),
        )
        .unwrap();
        client
            .save_tokens(Tokens {
                access: "access".into(),
                refresh: "refresh".into(),
                email: "e@example.com".into(),
            })
            .unwrap();
        let provider = BlueyManagedProvider::new(client, ManagedLane::Instant);
        let mut stream = provider
            .complete_stream(&LlmRequest {
                system: "system".into(),
                user: "user".into(),
                session_id: Some("sess-1".into()),
                max_tokens: Some(10),
                temperature: Some(0.2),
                reasoning_effort: None,
                thinking_budget_tokens: None,
                request_id: Some("req-done-before-billing".into()),
                image_data_urls: Vec::new(),
            })
            .await
            .unwrap();

        let first = stream.next().await.expect("first delta").unwrap();
        assert_eq!(first.text, "partial");
        let error = stream
            .next()
            .await
            .expect("done-before-billing error")
            .unwrap_err()
            .to_string();
        assert!(error.contains("final billing metadata"));
    }

    #[test]
    fn maps_capacity_busy_to_typed_llm_error() {
        let mapped = map_err(CloudError::CapacityBusy {
            retry_after_secs: 17,
            reason: "provider_key_cooling_down".into(),
        });

        match mapped {
            LlmError::CapacityBusy {
                retry_after_secs,
                reason,
            } => {
                assert_eq!(retry_after_secs, 17);
                assert_eq!(reason, "provider_key_cooling_down");
            }
            other => panic!("expected CapacityBusy, got {other:?}"),
        }
    }
}
