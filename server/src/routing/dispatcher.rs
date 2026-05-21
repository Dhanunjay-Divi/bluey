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
use serde::{Deserialize, Serialize};

use crate::config::UpstreamKeys;

/// Normalised completion response.
#[derive(Debug)]
pub struct Completion {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Resolve the lane/task to a concrete provider+model.
/// Mirrors `cue_router::policy::StaticPolicy::defaults`.
pub fn resolve_route(lane: &str) -> (&'static str, &'static str) {
    match lane {
        "instant" => ("openai", "gpt-4o-mini"),
        "deep" => ("anthropic", "claude-3-7-sonnet-latest"),
        "vision" => ("openai", "gpt-4o"),
        "local" => ("ollama", "llama3.1"),
        _ => ("anthropic", "claude-3-5-sonnet-latest"), // balanced default
    }
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
    // Codex S4.5: fallback estimate when upstream omits `usage`. The
    // server passes its entry-cost ceiling so we charge the best
    // available approximation rather than $0.
    fallback_input_tokens: Option<i64>,
) -> Result<Completion> {
    match provider {
        "openai" => {
            openai_complete(
                keys,
                model,
                system,
                user,
                max_tokens,
                temperature,
                fallback_input_tokens,
            )
            .await
        }
        "anthropic" => {
            anthropic_complete(
                keys,
                model,
                system,
                user,
                max_tokens,
                temperature,
                fallback_input_tokens,
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
    content: &'a str,
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
    keys: &UpstreamKeys,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    fallback_input_tokens: Option<i64>,
) -> Result<Completion> {
    let key = keys
        .openai_api_key
        .as_ref()
        .ok_or_else(|| anyhow!("OPENAI_API_KEY not configured on bluey-server"))?;
    let req = OpenAiChatReq {
        model,
        messages: vec![
            OpenAiMessage {
                role: "system",
                content: system,
            },
            OpenAiMessage {
                role: "user",
                content: user,
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
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("openai {status}: {body}"));
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

// ─── Anthropic ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct AnthropicReq<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
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
    keys: &UpstreamKeys,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    fallback_input_tokens: Option<i64>,
) -> Result<Completion> {
    let key = keys
        .anthropic_api_key
        .as_ref()
        .ok_or_else(|| anyhow!("ANTHROPIC_API_KEY not configured on bluey-server"))?;
    let req = AnthropicReq {
        model,
        max_tokens: max_tokens.unwrap_or(2048),
        system,
        messages: vec![AnthropicMessage {
            role: "user",
            content: user,
        }],
        temperature,
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
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("anthropic {status}: {body}"));
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

/// Codex Stage 12: OpenAI embeddings via /v1/embeddings.
pub async fn embed(
    keys: &UpstreamKeys,
    provider: &str,
    model: &str,
    input: &str,
) -> Result<EmbedCompletion> {
    match provider {
        "openai" => openai_embed(keys, model, input).await,
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

async fn openai_embed(keys: &UpstreamKeys, model: &str, input: &str) -> Result<EmbedCompletion> {
    let key = keys
        .openai_api_key
        .as_ref()
        .ok_or_else(|| anyhow!("OPENAI_API_KEY not configured on bluey-server"))?;
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
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("openai embed {status}: {body}"));
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
        "deepgram" => deepgram_transcribe(keys, model, audio_bytes, content_type).await,
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

async fn deepgram_transcribe(
    keys: &UpstreamKeys,
    model: &str,
    audio_bytes: &[u8],
    content_type: &str,
) -> Result<TranscribeCompletion> {
    let key = keys
        .deepgram_api_key
        .as_ref()
        .ok_or_else(|| anyhow!("DEEPGRAM_API_KEY not configured on bluey-server"))?;
    // POST to /v1/listen?model=...&punctuate=true with the raw audio
    // bytes as the body. Deepgram accepts audio/wav, audio/mpeg, etc.
    let url = format!(
        "https://api.deepgram.com/v1/listen?model={model}&punctuate=true&smart_format=true"
    );
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
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("deepgram {status}: {body}"));
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
        assert_eq!(resolve_route("local"), ("ollama", "llama3.1"));
        // Unknown → balanced default.
        assert_eq!(
            resolve_route("???"),
            ("anthropic", "claude-3-5-sonnet-latest"),
        );
    }
}
