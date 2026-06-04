//! Upstream provider proxying. Speaks the OpenAI Chat Completions API
//! and the Anthropic Messages API directly; returns a normalised
//! response with token counts.
//!
//! v0.2 scope: non-streaming. Streaming proxy lands later (R14.x).

fn override_url(default: &str, env_var: &str) -> String {
    std::env::var(env_var)
        .ok()
        .filter(|v| !v.is_empty())
        .map(|base| {
            format!(
                "{base}{}",
                &default[default.find("/v1").unwrap_or(default.len())..]
            )
        })
        .unwrap_or_else(|| default.to_string())
}

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::UpstreamKeys;

#[derive(Debug, Error)]
#[error("{provider} upstream http {status}")]
pub struct UpstreamHttpError {
    pub provider: String,
    pub status: u16,
    pub retry_after_secs: Option<u64>,
}

pub fn upstream_retry_after(error: &anyhow::Error) -> Option<u64> {
    error
        .downcast_ref::<UpstreamHttpError>()
        .and_then(|error| error.retry_after_secs)
}

fn upstream_http_error(
    provider: &str,
    status: reqwest::StatusCode,
    headers: &HeaderMap,
) -> anyhow::Error {
    anyhow!(UpstreamHttpError {
        provider: provider.to_string(),
        status: status.as_u16(),
        retry_after_secs: retry_after_secs(status, headers),
    })
}

fn retry_after_secs(status: reqwest::StatusCode, headers: &HeaderMap) -> Option<u64> {
    let explicit = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0);
    if explicit.is_some() {
        return explicit;
    }
    match status.as_u16() {
        429 | 529 => Some(default_capacity_cooldown_secs()),
        _ => None,
    }
}

fn default_capacity_cooldown_secs() -> u64 {
    std::env::var("BLUEY_PROVIDER_429_COOLDOWN_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(30)
}

/// Normalised completion response.
#[derive(Debug)]
pub struct Completion {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Provider-neutral reasoning control. The server maps this to whichever
/// upstream knob is safe for the selected provider/model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    Off,
    Low,
    Medium,
    High,
    Auto,
}

/// Per-request thinking budget. `max_tokens` is intentionally optional:
/// some providers expose effort tiers, some expose token budgets, and some
/// expose both on different API families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThinkingBudget {
    pub mode: ThinkingMode,
    pub max_tokens: Option<u32>,
}

impl ThinkingBudget {
    pub const fn off() -> Self {
        Self {
            mode: ThinkingMode::Off,
            max_tokens: None,
        }
    }

    fn is_enabled(self) -> bool {
        !matches!(self.mode, ThinkingMode::Off)
    }
}

/// Resolve a lane + optional client override into Bluey's server policy.
pub fn resolve_thinking_budget(
    lane: &str,
    requested_effort: Option<&str>,
    requested_tokens: Option<u32>,
) -> ThinkingBudget {
    let mut budget = default_thinking_budget(lane);

    if let Some(env_effort) = env_lane_value(lane, "EFFORT") {
        if let Some(mode) = parse_thinking_mode(&env_effort) {
            budget.mode = mode;
        }
    }
    if let Some(env_tokens) =
        env_lane_value(lane, "TOKENS").and_then(|value| value.parse::<u32>().ok())
    {
        budget.max_tokens = Some(clamp_thinking_tokens(env_tokens));
    }

    if let Some(mode) = requested_effort.and_then(parse_thinking_mode) {
        budget.mode = mode;
    }
    if let Some(tokens) = requested_tokens {
        budget.max_tokens = Some(clamp_thinking_tokens(tokens));
    }

    if matches!(budget.mode, ThinkingMode::Off) {
        budget.max_tokens = None;
    }
    budget
}

/// Max output token budget used for entry cost checks. Reasoning tokens count
/// against provider output budgets, so deep thinking must reserve room.
pub fn effective_max_output_tokens(requested: Option<u32>, thinking: ThinkingBudget) -> u32 {
    let base = requested.unwrap_or(2048).max(256);
    match (thinking.is_enabled(), thinking.max_tokens) {
        (true, Some(tokens)) => base.max(tokens.saturating_add(1024)),
        _ => base,
    }
}

fn default_thinking_budget(lane: &str) -> ThinkingBudget {
    match lane {
        "deep" => ThinkingBudget {
            mode: ThinkingMode::Medium,
            max_tokens: Some(4_096),
        },
        // Keep fast lanes fast by default. Callers/operators can opt in via
        // request fields or BLUEY_THINKING_* env vars.
        _ => ThinkingBudget::off(),
    }
}

fn parse_thinking_mode(value: &str) -> Option<ThinkingMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "none" | "0" | "false" => Some(ThinkingMode::Off),
        "low" | "1" => Some(ThinkingMode::Low),
        "medium" | "med" | "2" => Some(ThinkingMode::Medium),
        "high" | "3" => Some(ThinkingMode::High),
        "auto" | "dynamic" => Some(ThinkingMode::Auto),
        _ => None,
    }
}

fn env_lane_value(lane: &str, suffix: &str) -> Option<String> {
    let lane = lane
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    std::env::var(format!("BLUEY_THINKING_{lane}_{suffix}"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clamp_thinking_tokens(tokens: u32) -> u32 {
    tokens.clamp(1_024, 32_000)
}

/// Resolve the lane/task to a concrete provider+model.
///
/// This returns the preferred route for compatibility with older callers.
/// New managed paths should use `resolve_route_candidates()` so they can
/// skip an exhausted provider without failing the customer request.
pub fn resolve_route(lane: &str) -> (&'static str, &'static str) {
    if lane == "local" {
        return ("unsupported", "local");
    }
    resolve_route_candidates(lane)
        .into_iter()
        .next()
        .unwrap_or(("anthropic", "claude-3-5-sonnet-latest"))
}

/// Ordered fallback candidates for one lane.
///
/// The list is intentionally conservative: the first route preserves product
/// quality, later routes preserve availability. Pricing and provider capacity
/// are checked by the API layer before dispatch.
pub fn resolve_route_candidates(lane: &str) -> Vec<(&'static str, &'static str)> {
    match lane {
        "instant" => vec![
            ("openai", "gpt-4o-mini"),
            ("anthropic", "claude-3-5-sonnet-latest"),
        ],
        "deep" => vec![
            ("anthropic", "claude-3-7-sonnet-latest"),
            ("openai", "gpt-4o"),
            ("anthropic", "claude-3-5-sonnet-latest"),
        ],
        "vision" => vec![("openai", "gpt-4o")],
        // The managed cloud never dispatches local/on-device models. Local
        // fallback is selected in the daemon before traffic reaches
        // bluey-server.
        "local" => vec![],
        _ => vec![
            ("anthropic", "claude-3-5-sonnet-latest"),
            ("openai", "gpt-4o-mini"),
        ], // balanced default
    }
}

/// Ordered fallback candidates for chunked server-side STT.
pub fn resolve_transcribe_candidates(deepgram_model: Option<&str>) -> Vec<(&'static str, String)> {
    vec![
        ("deepgram", deepgram_model.unwrap_or("nova-3").to_string()),
        ("openai", "gpt-4o-mini-transcribe".to_string()),
    ]
}

/// Run a single non-streaming completion against the upstream provider.
#[allow(clippy::too_many_arguments)]
pub async fn complete(
    keys: &UpstreamKeys,
    provider: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    // Codex S4.5: fallback estimate when upstream omits `usage`. The
    // server passes its entry-cost ceiling so we charge the best
    // available approximation rather than $0.
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<Completion> {
    match provider {
        "openai" => {
            let key = keys
                .openai_key(&format!("chat:{model}:{system}:{user}"))
                .ok_or_else(|| anyhow!("OPENAI_API_KEY(S) not configured on bluey-server"))?;
            openai_complete(
                key,
                model,
                system,
                user,
                max_tokens,
                temperature,
                thinking,
                fallback_input_tokens,
                image_data_urls,
            )
            .await
        }
        "anthropic" => {
            let key = keys
                .anthropic_key(&format!("chat:{model}:{system}:{user}"))
                .ok_or_else(|| anyhow!("ANTHROPIC_API_KEY(S) not configured on bluey-server"))?;
            anthropic_complete(
                key,
                model,
                system,
                user,
                max_tokens,
                temperature,
                thinking,
                fallback_input_tokens,
                image_data_urls,
            )
            .await
        }
        // Codex S4.4: explicit failure for unsupported providers
        // including `ollama` (which only the daemon's local fallback
        // path should run).
        other => Err(anyhow!(
            "unsupported provider for managed dispatch: {other}"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn complete_with_key(
    api_key: &str,
    provider: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<Completion> {
    match provider {
        "openai" => {
            openai_complete(
                api_key,
                model,
                system,
                user,
                max_tokens,
                temperature,
                thinking,
                fallback_input_tokens,
                image_data_urls,
            )
            .await
        }
        "anthropic" => {
            anthropic_complete(
                api_key,
                model,
                system,
                user,
                max_tokens,
                temperature,
                thinking,
                fallback_input_tokens,
                image_data_urls,
            )
            .await
        }
        other => Err(anyhow!(
            "unsupported provider for managed dispatch: {other}"
        )),
    }
}

// ─── OpenAI ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAiChatReq<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'static str,
    content: OpenAiMessageContent<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OpenAiMessageContent<'a> {
    Text(&'a str),
    Parts(Vec<OpenAiContentPart<'a>>),
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum OpenAiContentPart<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAiImageUrl<'a> },
}

#[derive(Serialize)]
struct OpenAiImageUrl<'a> {
    url: &'a str,
}

#[derive(Deserialize)]
struct OpenAiChatResp {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[allow(clippy::too_many_arguments)]
async fn openai_complete(
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    _thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<Completion> {
    let user_content = openai_user_content(user, image_data_urls);
    let req = OpenAiChatReq {
        model,
        messages: vec![
            OpenAiMessage {
                role: "system",
                content: OpenAiMessageContent::Text(system),
            },
            OpenAiMessage {
                role: "user",
                content: user_content,
            },
        ],
        max_tokens,
        temperature,
    };
    let resp = reqwest::Client::new()
        .post(
            override_url(
                "https://api.openai.com/v1/chat/completions",
                "BLUEY_TEST_OPENAI_URL",
            )
            .as_str(),
        )
        .bearer_auth(key)
        .json(&req)
        .send()
        .await
        .context("openai http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("openai", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: OpenAiChatResp = resp.json().await.context("openai json")?;
    let text = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();
    let (input_tokens, output_tokens) = parsed
        .usage
        .map(|u| (u.prompt_tokens, u.completion_tokens))
        .unwrap_or((fallback_input_tokens.unwrap_or(0), 0));
    Ok(Completion {
        text,
        provider: "openai".to_string(),
        model: model.to_string(),
        input_tokens,
        output_tokens,
    })
}

fn openai_user_content<'a>(
    user: &'a str,
    image_data_urls: &'a [String],
) -> OpenAiMessageContent<'a> {
    if image_data_urls.is_empty() {
        return OpenAiMessageContent::Text(user);
    }

    let mut parts = Vec::with_capacity(image_data_urls.len() + 1);
    parts.push(OpenAiContentPart::Text { text: user });
    parts.extend(
        image_data_urls
            .iter()
            .map(|url| OpenAiContentPart::ImageUrl {
                image_url: OpenAiImageUrl { url },
            }),
    );
    OpenAiMessageContent::Parts(parts)
}

// ─── Anthropic ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct AnthropicReq<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinkingReq>,
}

#[derive(Serialize)]
struct AnthropicThinkingReq {
    #[serde(rename = "type")]
    ty: &'static str,
    budget_tokens: u32,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Deserialize)]
struct AnthropicResp {
    content: Vec<AnthropicContent>,
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(default)]
    text: String,
    #[serde(rename = "type")]
    _type: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: i64,
    output_tokens: i64,
}

#[allow(clippy::too_many_arguments)]
async fn anthropic_complete(
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<Completion> {
    if !image_data_urls.is_empty() {
        return Err(anyhow!(
            "image payloads are only supported by the OpenAI vision route"
        ));
    }

    let thinking_req = anthropic_thinking_for(model, thinking);
    let effective_max_tokens = effective_max_output_tokens(max_tokens, thinking);
    let req = AnthropicReq {
        model,
        max_tokens: effective_max_tokens,
        system,
        messages: vec![AnthropicMessage {
            role: "user",
            content: user,
        }],
        // Anthropic rejects temperature together with extended thinking.
        temperature: if thinking_req.is_some() {
            None
        } else {
            temperature
        },
        thinking: thinking_req,
    };
    let resp = reqwest::Client::new()
        .post(
            override_url(
                "https://api.anthropic.com/v1/messages",
                "BLUEY_TEST_ANTHROPIC_URL",
            )
            .as_str(),
        )
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&req)
        .send()
        .await
        .context("anthropic http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("anthropic", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: AnthropicResp = resp.json().await.context("anthropic json")?;
    let text = parsed
        .content
        .into_iter()
        .map(|c| c.text)
        .collect::<Vec<_>>()
        .join("");
    Ok(Completion {
        text,
        provider: "anthropic".to_string(),
        model: model.to_string(),
        input_tokens: parsed
            .usage
            .as_ref()
            .map(|u| u.input_tokens)
            .unwrap_or_else(|| fallback_input_tokens.unwrap_or(0)),
        output_tokens: parsed.usage.as_ref().map(|u| u.output_tokens).unwrap_or(0),
    })
}

fn anthropic_thinking_for(model: &str, thinking: ThinkingBudget) -> Option<AnthropicThinkingReq> {
    if !thinking.is_enabled() || !anthropic_supports_manual_thinking(model) {
        return None;
    }
    let default_tokens = match thinking.mode {
        ThinkingMode::Off => return None,
        ThinkingMode::Low => 1_024,
        ThinkingMode::Medium | ThinkingMode::Auto => 4_096,
        ThinkingMode::High => 8_192,
    };
    Some(AnthropicThinkingReq {
        ty: "enabled",
        budget_tokens: thinking.max_tokens.unwrap_or(default_tokens),
    })
}

fn anthropic_supports_manual_thinking(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    // Current production deep lane is Claude 3.7 Sonnet. Newer Claude 4.x
    // models prefer adaptive thinking on Anthropic's side, so keep manual
    // budget opt-in narrow until the route table moves to those APIs.
    model.contains("claude-3-7")
}

/// Codex Stage 12: OpenAI embeddings via /v1/embeddings.
pub async fn embed(
    keys: &UpstreamKeys,
    provider: &str,
    model: &str,
    input: &str,
) -> Result<EmbedCompletion> {
    match provider {
        "openai" => {
            let key = keys
                .openai_key(&format!("embed:{model}:{input}"))
                .ok_or_else(|| anyhow!("OPENAI_API_KEY(S) not configured on bluey-server"))?;
            openai_embed(key, model, input).await
        }
        other => Err(anyhow!(
            "unsupported embedding provider for managed dispatch: {other}"
        )),
    }
}

pub async fn embed_with_key(
    api_key: &str,
    provider: &str,
    model: &str,
    input: &str,
) -> Result<EmbedCompletion> {
    match provider {
        "openai" => openai_embed(api_key, model, input).await,
        other => Err(anyhow!(
            "unsupported embedding provider for managed dispatch: {other}"
        )),
    }
}

/// One embedding response. `vector` is float32, length depends on model
/// (text-embedding-3-small returns 1536-dim by default).
#[derive(Debug, Clone)]
pub struct EmbedCompletion {
    pub vector: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
}

#[derive(serde::Serialize)]
struct OpenAiEmbedReq<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct OpenAiEmbedResp {
    data: Vec<OpenAiEmbedData>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiEmbedData {
    embedding: Vec<f32>,
}

async fn openai_embed(key: &str, model: &str, input: &str) -> Result<EmbedCompletion> {
    let req = OpenAiEmbedReq { model, input };
    let resp = reqwest::Client::new()
        .post(
            override_url(
                "https://api.openai.com/v1/embeddings",
                "BLUEY_TEST_OPENAI_URL",
            )
            .as_str(),
        )
        .bearer_auth(key)
        .json(&req)
        .send()
        .await
        .context("openai embed http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("openai", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: OpenAiEmbedResp = resp.json().await.context("openai embed json")?;
    let vector = parsed
        .data
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("openai embed: no data returned"))?
        .embedding;
    let input_tokens = parsed
        .usage
        .map(|u| u.prompt_tokens)
        .unwrap_or((input.len() as i64) / 4);
    Ok(EmbedCompletion {
        vector,
        provider: "openai".to_string(),
        model: model.to_string(),
        input_tokens,
    })
}

/// Codex Stage 12b: Deepgram transcription.
pub async fn transcribe(
    keys: &UpstreamKeys,
    provider: &str,
    model: &str,
    audio_bytes: &[u8],
    content_type: &str,
) -> Result<TranscribeCompletion> {
    match provider {
        "deepgram" => {
            let key = keys
                .deepgram_key(&format!("transcribe:{model}:{}", audio_bytes.len()))
                .ok_or_else(|| anyhow!("DEEPGRAM_API_KEY(S) not configured on bluey-server"))?;
            deepgram_transcribe(key, model, audio_bytes, content_type).await
        }
        "openai" => {
            let key = keys
                .openai_key(&format!("transcribe:{model}:{}", audio_bytes.len()))
                .ok_or_else(|| anyhow!("OPENAI_API_KEY(S) not configured on bluey-server"))?;
            openai_transcribe(key, model, audio_bytes, content_type).await
        }
        other => Err(anyhow!(
            "unsupported transcribe provider for managed dispatch: {other}"
        )),
    }
}

pub async fn transcribe_with_key(
    api_key: &str,
    provider: &str,
    model: &str,
    audio_bytes: &[u8],
    content_type: &str,
) -> Result<TranscribeCompletion> {
    match provider {
        "deepgram" => deepgram_transcribe(api_key, model, audio_bytes, content_type).await,
        "openai" => openai_transcribe(api_key, model, audio_bytes, content_type).await,
        other => Err(anyhow!(
            "unsupported transcribe provider for managed dispatch: {other}"
        )),
    }
}

#[derive(Debug, Clone)]
pub struct TranscribeCompletion {
    pub text: String,
    pub provider: String,
    pub model: String,
    /// Audio duration in seconds (used as "input tokens" for billing).
    pub duration_seconds: i64,
}

#[derive(Deserialize)]
struct DeepgramResp {
    metadata: Option<DeepgramMetadata>,
    results: DeepgramResults,
}

#[derive(Deserialize)]
struct DeepgramMetadata {
    duration: Option<f64>,
}

#[derive(Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Deserialize)]
struct DeepgramAlternative {
    transcript: String,
}

#[derive(Deserialize)]
struct OpenAiTranscribeResp {
    text: String,
}

async fn deepgram_transcribe(
    key: &str,
    model: &str,
    audio_bytes: &[u8],
    content_type: &str,
) -> Result<TranscribeCompletion> {
    // POST to /v1/listen?model=...&punctuate=true with the raw audio
    // bytes as the body. Deepgram accepts audio/wav, audio/mpeg, etc.
    let default_url = format!(
        "https://api.deepgram.com/v1/listen?model={model}&punctuate=true&smart_format=true"
    );
    let url = override_url(&default_url, "BLUEY_TEST_DEEPGRAM_URL");
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", format!("Token {key}"))
        .header("Content-Type", content_type)
        .body(audio_bytes.to_vec())
        .send()
        .await
        .context("deepgram transcribe http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("deepgram", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: DeepgramResp = resp.json().await.context("deepgram json")?;
    let text = parsed
        .results
        .channels
        .into_iter()
        .next()
        .and_then(|c| c.alternatives.into_iter().next())
        .map(|a| a.transcript)
        .unwrap_or_default();
    let duration_seconds = parsed
        .metadata
        .and_then(|m| m.duration)
        .map(|d| d.ceil() as i64)
        .unwrap_or(0)
        .max(1); // bill minimum 1 second
    Ok(TranscribeCompletion {
        text,
        provider: "deepgram".to_string(),
        model: model.to_string(),
        duration_seconds,
    })
}

async fn openai_transcribe(
    key: &str,
    model: &str,
    audio_bytes: &[u8],
    content_type: &str,
) -> Result<TranscribeCompletion> {
    let file = Part::bytes(audio_bytes.to_vec())
        .file_name(audio_filename(content_type))
        .mime_str(content_type)
        .context("openai transcribe mime")?;
    let form = Form::new()
        .text("model", model.to_string())
        .part("file", file);
    let resp = reqwest::Client::new()
        .post(
            override_url(
                "https://api.openai.com/v1/audio/transcriptions",
                "BLUEY_TEST_OPENAI_URL",
            )
            .as_str(),
        )
        .bearer_auth(key)
        .multipart(form)
        .send()
        .await
        .context("openai transcribe http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("openai", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: OpenAiTranscribeResp = resp.json().await.context("openai transcribe json")?;
    Ok(TranscribeCompletion {
        text: parsed.text,
        provider: "openai".to_string(),
        model: model.to_string(),
        duration_seconds: estimate_audio_seconds(audio_bytes),
    })
}

fn estimate_audio_seconds(audio_bytes: &[u8]) -> i64 {
    // Chunked REST STT receives mixed compressed/uncompressed formats. This is
    // only used when a provider does not return duration metadata.
    (audio_bytes.len() as i64 / 16_000).max(1)
}

fn audio_filename(content_type: &str) -> &'static str {
    match content_type.split(';').next().unwrap_or("").trim() {
        "audio/mpeg" | "audio/mp3" => "audio.mp3",
        "audio/mp4" | "audio/x-m4a" => "audio.m4a",
        "audio/webm" => "audio.webm",
        "audio/ogg" => "audio.ogg",
        _ => "audio.wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_route_known_lanes() {
        assert_eq!(resolve_route("instant"), ("openai", "gpt-4o-mini"));
        assert_eq!(
            resolve_route("balanced"),
            ("anthropic", "claude-3-5-sonnet-latest"),
        );
        assert_eq!(
            resolve_route("deep"),
            ("anthropic", "claude-3-7-sonnet-latest"),
        );
        assert_eq!(resolve_route("vision"), ("openai", "gpt-4o"));
        assert_eq!(resolve_route("local"), ("unsupported", "local"));
        // Unknown → balanced default.
        assert_eq!(
            resolve_route("???"),
            ("anthropic", "claude-3-5-sonnet-latest"),
        );
    }

    #[test]
    fn route_candidates_preserve_preferred_first() {
        assert_eq!(
            resolve_route_candidates("deep"),
            vec![
                ("anthropic", "claude-3-7-sonnet-latest"),
                ("openai", "gpt-4o"),
                ("anthropic", "claude-3-5-sonnet-latest")
            ]
        );
        assert_eq!(
            resolve_route_candidates("vision"),
            vec![("openai", "gpt-4o")]
        );
        assert!(
            resolve_route_candidates("local").is_empty(),
            "managed cloud must not dispatch daemon-only local lanes"
        );
    }

    #[test]
    fn transcribe_candidates_prefer_deepgram_then_openai() {
        assert_eq!(
            resolve_transcribe_candidates(None),
            vec![
                ("deepgram", "nova-3".to_string()),
                ("openai", "gpt-4o-mini-transcribe".to_string())
            ]
        );
        assert_eq!(
            resolve_transcribe_candidates(Some("nova-2"))[0],
            ("deepgram", "nova-2".to_string())
        );
    }

    #[test]
    fn openai_user_content_serializes_image_parts() {
        let images = vec!["data:image/png;base64,aGVsbG8=".to_string()];
        let content = openai_user_content("What is on screen?", &images);
        let value = serde_json::to_value(OpenAiMessage {
            role: "user",
            content,
        })
        .unwrap();

        assert_eq!(value["role"], "user");
        assert_eq!(value["content"][0]["type"], "text");
        assert_eq!(value["content"][0]["text"], "What is on screen?");
        assert_eq!(value["content"][1]["type"], "image_url");
        assert_eq!(
            value["content"][1]["image_url"]["url"],
            "data:image/png;base64,aGVsbG8="
        );
    }

    #[tokio::test]
    async fn anthropic_rejects_image_payloads() {
        let images = vec!["data:image/png;base64,aGVsbG8=".to_string()];
        let err = anthropic_complete(
            "sk-test",
            "claude-3-5-sonnet-latest",
            "system",
            "user",
            None,
            None,
            ThinkingBudget::off(),
            None,
            &images,
        )
        .await
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("image payloads are only supported by the OpenAI vision route"));
    }

    #[test]
    fn deep_lane_gets_default_thinking_budget() {
        let budget = resolve_thinking_budget("deep", None, None);
        assert_eq!(budget.mode, ThinkingMode::Medium);
        assert_eq!(budget.max_tokens, Some(4096));
        assert_eq!(effective_max_output_tokens(None, budget), 5120);
    }

    #[test]
    fn instant_lane_defaults_to_no_thinking() {
        let budget = resolve_thinking_budget("instant", None, None);
        assert_eq!(budget, ThinkingBudget::off());
        assert_eq!(effective_max_output_tokens(Some(512), budget), 512);
    }

    #[test]
    fn request_can_override_thinking_budget() {
        let budget = resolve_thinking_budget("balanced", Some("high"), Some(9000));
        assert_eq!(budget.mode, ThinkingMode::High);
        assert_eq!(budget.max_tokens, Some(9000));
        assert_eq!(effective_max_output_tokens(Some(2048), budget), 10024);
    }

    #[test]
    fn anthropic_manual_thinking_only_for_known_supported_models() {
        let budget = resolve_thinking_budget("deep", None, None);
        let enabled = anthropic_thinking_for("claude-3-7-sonnet-latest", budget).unwrap();
        assert_eq!(enabled.ty, "enabled");
        assert_eq!(enabled.budget_tokens, 4096);
        assert!(anthropic_thinking_for("claude-3-5-sonnet-latest", budget).is_none());
        assert!(anthropic_thinking_for("gpt-4o", budget).is_none());
    }
}
