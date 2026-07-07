//! Upstream provider proxying. Speaks the OpenAI Chat Completions API
//! and the Anthropic Messages API directly; returns a normalised
//! response with token counts.
//!
//! v0.2 scope: non-streaming and streaming managed completions. The server
//! streams provider deltas to the desktop, then emits one final billing event
//! after the same post-completion charging/idempotency path succeeds.

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

fn override_direct_url(default: &str, env_var: &str) -> String {
    std::env::var(env_var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use futures_util::Stream;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use thiserror::Error;

use crate::config::UpstreamKeys;

const OPENAI_FAST_MODEL: &str = "gpt-5.4-mini";
const OPENAI_ACCURATE_MODEL: &str = "gpt-5.5";
const ANTHROPIC_BALANCED_MODEL: &str = "claude-sonnet-4-6";
const ANTHROPIC_DEEP_MODEL: &str = "claude-opus-4-8";
const ANTHROPIC_FAST_MODEL: &str = "claude-haiku-4-5-20251001";
const GEMINI_PRO_MODEL: &str = "gemini-3.1-pro-preview";
const GEMINI_FLASH_MODEL: &str = "gemini-3-flash-preview";
const GEMINI_LITE_MODEL: &str = "gemini-3.1-flash-lite";
const DEEPSEEK_PRO_MODEL: &str = "deepseek-v4-pro";
const DEEPSEEK_FLASH_MODEL: &str = "deepseek-v4-flash";
const ZAI_FLAGSHIP_MODEL: &str = "glm-5.2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutePolicy {
    ProviderMix,
    QualityFirst,
    CostOptimized,
}

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
        .and_then(parse_retry_after_value);
    if explicit.is_some() {
        return explicit;
    }
    match status.as_u16() {
        429 | 529 => Some(default_capacity_cooldown_secs()),
        _ => None,
    }
}

fn parse_retry_after_value(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return (seconds > 0).then_some(seconds);
    }
    let deadline = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let seconds = (deadline - Utc::now()).num_seconds();
    (seconds > 0).then_some(seconds as u64)
}

fn default_capacity_cooldown_secs() -> u64 {
    std::env::var("BLUEY_PROVIDER_429_COOLDOWN_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(2)
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

/// Provider-neutral event returned by a streaming upstream completion.
#[derive(Debug)]
pub enum CompletionStreamEvent {
    Delta(String),
    Done {
        input_tokens: i64,
        output_tokens: i64,
    },
}

pub type CompletionEventStream =
    Pin<Box<dyn Stream<Item = Result<CompletionStreamEvent>> + Send + 'static>>;

pub struct StreamingCompletion {
    pub provider: String,
    pub model: String,
    pub events: CompletionEventStream,
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
/// Non-thinking output-token ceiling. Bounds per-request TPM reservation
/// (fewer provider 429s under a token/min limit) and worst-case per-request
/// cost. Thinking/deep lanes are exempt because their budget legitimately
/// needs the room. Override with BLUEY_MAX_OUTPUT_TOKENS.
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 2048;

fn non_thinking_output_ceiling() -> u32 {
    std::env::var("BLUEY_MAX_OUTPUT_TOKENS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|t| *t >= 256)
        .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
}

pub fn effective_max_output_tokens(requested: Option<u32>, thinking: ThinkingBudget) -> u32 {
    let base = requested.unwrap_or(2048).max(256);
    match (thinking.is_enabled(), thinking.max_tokens) {
        // Thinking/deep lanes keep their (larger) computed budget.
        (true, Some(tokens)) => base.max(tokens.saturating_add(1024)),
        (true, None) => base,
        // Non-thinking lanes (instant/balanced): clamp to the ceiling so a
        // client cannot reserve an unbounded output budget.
        (false, _) => base.min(non_thinking_output_ceiling()),
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
        .unwrap_or(("anthropic", ANTHROPIC_BALANCED_MODEL))
}

pub fn resolve_route_candidates(lane: &str) -> Vec<(&'static str, &'static str)> {
    resolve_route_candidates_with_seed(lane, "")
}

pub fn resolve_route_candidates_with_seed(
    lane: &str,
    seed: &str,
) -> Vec<(&'static str, &'static str)> {
    resolve_route_candidates_for_policy_and_seed(lane, route_policy(), seed)
}

fn route_policy() -> RoutePolicy {
    let raw = std::env::var("BLUEY_ROUTE_POLICY")
        .or_else(|_| std::env::var("BLUEY_ROUTE_ORDER"))
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    match raw.as_str() {
        "cost" | "cost_first" | "cost_optimized" | "cheap" | "glm" | "deepseek" => {
            RoutePolicy::CostOptimized
        }
        "quality" | "quality_first" | "static" | "legacy" => RoutePolicy::QualityFirst,
        "mix" | "mixed" | "provider_mix" | "balanced_mix" | "anti_429" | "capacity_mix" => {
            RoutePolicy::ProviderMix
        }
        _ => RoutePolicy::ProviderMix,
    }
}

/// Ordered fallback candidates for one lane.
///
/// The default provider-mix policy rotates the top tier by request id so bursts
/// do not all start on the same upstream. Operators can force the older static
/// quality order with `BLUEY_ROUTE_POLICY=quality_first`, or opt into
/// `BLUEY_ROUTE_POLICY=cost_optimized` to live-smoke cheaper GLM/DeepSeek text
/// lanes first. Pricing and provider capacity are checked by the API layer
/// before dispatch.
fn resolve_route_candidates_for_policy_and_seed(
    lane: &str,
    policy: RoutePolicy,
    seed: &str,
) -> Vec<(&'static str, &'static str)> {
    match (policy, lane) {
        (RoutePolicy::ProviderMix, lane) => resolve_provider_mix_candidates(lane, seed),
        (RoutePolicy::CostOptimized, "instant") => vec![
            ("deepseek", DEEPSEEK_FLASH_MODEL),
            ("gemini", GEMINI_LITE_MODEL),
            ("openai", OPENAI_FAST_MODEL),
            ("anthropic", ANTHROPIC_FAST_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
            ("anthropic", ANTHROPIC_BALANCED_MODEL),
        ],
        (RoutePolicy::CostOptimized, "deep") => vec![
            ("zai", ZAI_FLAGSHIP_MODEL),
            ("deepseek", DEEPSEEK_PRO_MODEL),
            ("anthropic", ANTHROPIC_DEEP_MODEL),
            ("gemini", GEMINI_PRO_MODEL),
            ("openai", OPENAI_ACCURATE_MODEL),
            ("anthropic", ANTHROPIC_BALANCED_MODEL),
            ("deepseek", DEEPSEEK_FLASH_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
        ],
        (RoutePolicy::CostOptimized, "vision") => vec![
            ("openai", OPENAI_ACCURATE_MODEL),
            ("gemini", GEMINI_PRO_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
            ("openai", OPENAI_FAST_MODEL),
        ],
        (RoutePolicy::CostOptimized, "local") => vec![],
        (RoutePolicy::CostOptimized, _) => vec![
            ("zai", ZAI_FLAGSHIP_MODEL),
            ("deepseek", DEEPSEEK_FLASH_MODEL),
            ("anthropic", ANTHROPIC_BALANCED_MODEL),
            ("gemini", GEMINI_PRO_MODEL),
            ("openai", OPENAI_ACCURATE_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
            ("openai", OPENAI_FAST_MODEL),
        ],
        (RoutePolicy::QualityFirst, "instant") => vec![
            ("openai", OPENAI_FAST_MODEL),
            ("deepseek", DEEPSEEK_FLASH_MODEL),
            ("gemini", GEMINI_LITE_MODEL),
            ("anthropic", ANTHROPIC_FAST_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
            ("anthropic", ANTHROPIC_BALANCED_MODEL),
        ],
        (RoutePolicy::QualityFirst, "deep") => vec![
            ("anthropic", ANTHROPIC_DEEP_MODEL),
            ("zai", ZAI_FLAGSHIP_MODEL),
            ("deepseek", DEEPSEEK_PRO_MODEL),
            ("gemini", GEMINI_PRO_MODEL),
            ("openai", OPENAI_ACCURATE_MODEL),
            ("anthropic", ANTHROPIC_BALANCED_MODEL),
            ("deepseek", DEEPSEEK_FLASH_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
        ],
        (RoutePolicy::QualityFirst, "vision") => vec![
            ("openai", OPENAI_ACCURATE_MODEL),
            ("gemini", GEMINI_PRO_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
            ("openai", OPENAI_FAST_MODEL),
        ],
        (RoutePolicy::QualityFirst, "local") => vec![],
        (RoutePolicy::QualityFirst, _) => vec![
            ("anthropic", ANTHROPIC_BALANCED_MODEL),
            ("deepseek", DEEPSEEK_FLASH_MODEL),
            ("zai", ZAI_FLAGSHIP_MODEL),
            ("gemini", GEMINI_PRO_MODEL),
            ("openai", OPENAI_ACCURATE_MODEL),
            ("gemini", GEMINI_FLASH_MODEL),
            ("openai", OPENAI_FAST_MODEL),
        ],
    }
}

fn resolve_provider_mix_candidates(lane: &str, seed: &str) -> Vec<(&'static str, &'static str)> {
    match lane {
        "instant" => rotate_preferred_routes(
            vec![
                ("openai", OPENAI_FAST_MODEL),
                ("deepseek", DEEPSEEK_FLASH_MODEL),
                ("gemini", GEMINI_LITE_MODEL),
                ("anthropic", ANTHROPIC_FAST_MODEL),
                ("zai", ZAI_FLAGSHIP_MODEL),
            ],
            vec![
                ("gemini", GEMINI_FLASH_MODEL),
                ("anthropic", ANTHROPIC_BALANCED_MODEL),
            ],
            lane,
            seed,
        ),
        "deep" => rotate_preferred_routes(
            vec![
                ("anthropic", ANTHROPIC_DEEP_MODEL),
                ("zai", ZAI_FLAGSHIP_MODEL),
                ("deepseek", DEEPSEEK_PRO_MODEL),
                ("gemini", GEMINI_PRO_MODEL),
                ("openai", OPENAI_ACCURATE_MODEL),
            ],
            vec![
                ("anthropic", ANTHROPIC_BALANCED_MODEL),
                ("deepseek", DEEPSEEK_FLASH_MODEL),
                ("gemini", GEMINI_FLASH_MODEL),
            ],
            lane,
            seed,
        ),
        "vision" => rotate_preferred_routes(
            vec![
                ("gemini", GEMINI_FLASH_MODEL),
                ("openai", OPENAI_ACCURATE_MODEL),
            ],
            vec![("gemini", GEMINI_PRO_MODEL), ("openai", OPENAI_FAST_MODEL)],
            lane,
            seed,
        ),
        "local" => vec![],
        _ => rotate_preferred_routes(
            vec![
                ("anthropic", ANTHROPIC_BALANCED_MODEL),
                ("deepseek", DEEPSEEK_FLASH_MODEL),
                ("zai", ZAI_FLAGSHIP_MODEL),
                ("gemini", GEMINI_PRO_MODEL),
                ("openai", OPENAI_ACCURATE_MODEL),
            ],
            vec![
                ("gemini", GEMINI_FLASH_MODEL),
                ("openai", OPENAI_FAST_MODEL),
            ],
            lane,
            seed,
        ),
    }
}

fn rotate_preferred_routes(
    mut preferred: Vec<(&'static str, &'static str)>,
    fallback: Vec<(&'static str, &'static str)>,
    lane: &str,
    seed: &str,
) -> Vec<(&'static str, &'static str)> {
    if preferred.len() > 1 && !seed.trim().is_empty() {
        let bucket = stable_route_bucket(seed, lane, preferred.len());
        preferred.rotate_left(bucket);
    }
    preferred.into_iter().chain(fallback).collect()
}

fn stable_route_bucket(seed: &str, lane: &str, modulo: usize) -> usize {
    if modulo <= 1 {
        return 0;
    }
    let mut hash = 0xcbf29ce484222325u64;
    for byte in seed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash ^= u64::from(b':');
    hash = hash.wrapping_mul(0x100000001b3);
    for byte in lane.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash % modulo as u64) as usize
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
        "gemini" => {
            let key = keys
                .gemini_key(&format!("chat:{model}:{system}:{user}"))
                .ok_or_else(|| anyhow!("GEMINI_API_KEY(S) not configured on bluey-server"))?;
            gemini_complete(
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
        "deepseek" => {
            let key = keys
                .deepseek_key(&format!("chat:{model}:{system}:{user}"))
                .ok_or_else(|| anyhow!("DEEPSEEK_API_KEY(S) not configured on bluey-server"))?;
            openai_compatible_complete(
                "deepseek",
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
        "zai" => {
            let key = keys
                .zai_key(&format!("chat:{model}:{system}:{user}"))
                .ok_or_else(|| anyhow!("ZAI_API_KEY(S) not configured on bluey-server"))?;
            openai_compatible_complete(
                "zai",
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
        "gemini" => {
            gemini_complete(
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
        "deepseek" | "zai" => {
            openai_compatible_complete(
                provider,
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

#[allow(clippy::too_many_arguments)]
pub async fn complete_stream_with_key(
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
) -> Result<StreamingCompletion> {
    match provider {
        "openai" => {
            openai_complete_stream(
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
            anthropic_complete_stream(
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
        "gemini" => {
            gemini_complete_stream(
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
        "deepseek" | "zai" => {
            openai_compatible_complete_stream(
                provider,
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
            "unsupported provider for managed streaming dispatch: {other}"
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
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<OpenAiStreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<OpenAiCompatibleThinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'static str>,
}

#[derive(Serialize)]
struct OpenAiStreamOptions {
    include_usage: bool,
}

#[derive(Serialize)]
struct OpenAiCompatibleThinking {
    #[serde(rename = "type")]
    ty: &'static str,
}

fn openai_token_limit_fields(model: &str, max_tokens: Option<u32>) -> (Option<u32>, Option<u32>) {
    if model.to_ascii_lowercase().starts_with("gpt-5") {
        (None, max_tokens)
    } else {
        (max_tokens, None)
    }
}

fn openai_effective_token_limit_fields(
    model: &str,
    max_tokens: Option<u32>,
    thinking: ThinkingBudget,
) -> (Option<u32>, Option<u32>) {
    openai_token_limit_fields(
        model,
        Some(effective_max_output_tokens(max_tokens, thinking)),
    )
}

fn openai_compatible_chat_url(provider: &str) -> Result<String> {
    match provider {
        "openai" => Ok(override_url(
            "https://api.openai.com/v1/chat/completions",
            "BLUEY_TEST_OPENAI_URL",
        )),
        "deepseek" => Ok(override_direct_url(
            "https://api.deepseek.com/chat/completions",
            "BLUEY_TEST_DEEPSEEK_URL",
        )),
        "zai" => Ok(override_direct_url(
            "https://api.z.ai/api/paas/v4/chat/completions",
            "BLUEY_TEST_ZAI_URL",
        )),
        other => Err(anyhow!("unsupported OpenAI-compatible provider: {other}")),
    }
}

fn openai_compatible_thinking_for(
    provider: &str,
    thinking: ThinkingBudget,
) -> (Option<OpenAiCompatibleThinking>, Option<&'static str>) {
    if !matches!(provider, "deepseek" | "zai") {
        return (None, None);
    }
    match thinking.mode {
        ThinkingMode::Off => (Some(OpenAiCompatibleThinking { ty: "disabled" }), None),
        ThinkingMode::High => (
            Some(OpenAiCompatibleThinking { ty: "enabled" }),
            Some("max"),
        ),
        ThinkingMode::Low | ThinkingMode::Medium | ThinkingMode::Auto => (
            Some(OpenAiCompatibleThinking { ty: "enabled" }),
            Some("high"),
        ),
    }
}

fn openai_compatible_temperature_for(
    provider: &str,
    model: &str,
    temperature: Option<f32>,
) -> Option<f32> {
    if provider == "openai" && model.to_ascii_lowercase().starts_with("gpt-5") {
        return None;
    }
    temperature
}

fn estimated_tokens_from_chars(chars: usize) -> i64 {
    if chars == 0 {
        return 0;
    }
    ((chars as i64) + 2) / 3
}

fn openai_stream_usage_or_estimate(
    provider: &str,
    model: &str,
    seen_done: bool,
    final_usage: Option<OpenAiUsage>,
    output_chars: usize,
    fallback_input_tokens: Option<i64>,
) -> Result<OpenAiUsage> {
    if !seen_done {
        return Err(anyhow!("{provider} stream ended before [DONE]"));
    }
    Ok(final_usage.unwrap_or_else(|| {
        tracing::warn!(
            provider,
            model,
            "OpenAI-compatible stream ended without final usage; using token estimate"
        );
        OpenAiUsage {
            prompt_tokens: fallback_input_tokens.unwrap_or(0),
            completion_tokens: estimated_tokens_from_chars(output_chars),
        }
    }))
}

fn anthropic_stream_usage_or_estimate(
    model: &str,
    seen_stop: bool,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    output_chars: usize,
    fallback_input: i64,
) -> Result<(i64, i64)> {
    if !seen_stop {
        return Err(anyhow!("anthropic stream ended before message_stop"));
    }
    let output_tokens = output_tokens.unwrap_or_else(|| {
        tracing::warn!(
            provider = "anthropic",
            model,
            "Anthropic stream ended without final usage; using token estimate"
        );
        estimated_tokens_from_chars(output_chars)
    });
    Ok((input_tokens.unwrap_or(fallback_input), output_tokens))
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
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: i64,
    #[serde(default)]
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
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<Completion> {
    openai_compatible_complete(
        "openai",
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

#[allow(clippy::too_many_arguments)]
async fn openai_compatible_complete(
    provider: &str,
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
    let user_content = openai_user_content(user, image_data_urls);
    let (max_tokens, max_completion_tokens) =
        openai_effective_token_limit_fields(model, max_tokens, thinking);
    let (thinking, reasoning_effort) = openai_compatible_thinking_for(provider, thinking);
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
        max_completion_tokens,
        temperature: openai_compatible_temperature_for(provider, model, temperature),
        stream: None,
        stream_options: None,
        thinking,
        reasoning_effort,
    };
    let resp = reqwest::Client::new()
        .post(openai_compatible_chat_url(provider)?.as_str())
        .bearer_auth(key)
        .json(&req)
        .send()
        .await
        .with_context(|| format!("{provider} http"))?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error(provider, status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: OpenAiChatResp = resp
        .json()
        .await
        .with_context(|| format!("{provider} json"))?;
    let text = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();
    let (input_tokens, output_tokens) = parsed
        .usage
        .map(|u| (u.prompt_tokens, u.completion_tokens))
        .unwrap_or_else(|| {
            (
                fallback_input_tokens.unwrap_or(0),
                estimated_tokens_from_chars(text.chars().count()),
            )
        });
    Ok(Completion {
        text,
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens,
        output_tokens,
    })
}

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    usage: Option<OpenAiUsage>,
    error: Option<OpenAiStreamError>,
}

#[derive(Deserialize)]
struct OpenAiStreamError {
    message: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
}

#[derive(Deserialize)]
struct OpenAiStreamDelta {
    content: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn openai_complete_stream(
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<StreamingCompletion> {
    openai_compatible_complete_stream(
        "openai",
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

#[allow(clippy::too_many_arguments)]
async fn openai_compatible_complete_stream(
    provider: &str,
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<StreamingCompletion> {
    let user_content = openai_user_content(user, image_data_urls);
    let (max_tokens, max_completion_tokens) =
        openai_effective_token_limit_fields(model, max_tokens, thinking);
    let (thinking, reasoning_effort) = openai_compatible_thinking_for(provider, thinking);
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
        max_completion_tokens,
        temperature: openai_compatible_temperature_for(provider, model, temperature),
        stream: Some(true),
        stream_options: Some(OpenAiStreamOptions {
            include_usage: true,
        }),
        thinking,
        reasoning_effort,
    };
    let resp = reqwest::Client::new()
        .post(openai_compatible_chat_url(provider)?.as_str())
        .bearer_auth(key)
        .json(&req)
        .send()
        .await
        .with_context(|| format!("{provider} stream http"))?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error(provider, status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }

    let provider_string = provider.to_string();
    let model_string = model.to_string();
    let stream_provider = provider_string.clone();
    let stream_model = model_string.clone();
    let mut bytes = resp.bytes_stream();
    let stream = async_stream::try_stream! {
        let mut buffer = String::new();
        let mut pending_utf8 = Vec::new();
        let mut seen_done = false;
        let mut final_usage: Option<OpenAiUsage> = None;
        let mut output_chars: usize = 0;

        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.with_context(|| format!("{stream_provider} stream read"))?;
            append_utf8_chunk(&chunk, &mut pending_utf8, &mut buffer)?;
            while let Some((_, data)) = take_sse_event(&mut buffer) {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                if data == "[DONE]" {
                    seen_done = true;
                    continue;
                }
                for delta in
                    parse_openai_stream_chunk(stream_provider.as_str(), data, &mut final_usage)?
                {
                    output_chars = output_chars.saturating_add(delta.chars().count());
                    yield CompletionStreamEvent::Delta(delta);
                }
            }
        }
        if !pending_utf8.is_empty() {
            let tail = std::str::from_utf8(&pending_utf8)
                .with_context(|| format!("{stream_provider} stream trailing utf8"))?;
            buffer.push_str(tail);
        }
        while let Some((_, data)) = take_sse_event(&mut buffer) {
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                seen_done = seen_done || data == "[DONE]";
                continue;
            }
            for delta in
                parse_openai_stream_chunk(stream_provider.as_str(), data, &mut final_usage)?
            {
                output_chars = output_chars.saturating_add(delta.chars().count());
                yield CompletionStreamEvent::Delta(delta);
            }
        }
        let usage = openai_stream_usage_or_estimate(
            &stream_provider,
            &stream_model,
            seen_done,
            final_usage,
            output_chars,
            fallback_input_tokens,
        )?;
        let (input_tokens, output_tokens) = (usage.prompt_tokens, usage.completion_tokens);
        yield CompletionStreamEvent::Done {
            input_tokens,
            output_tokens,
        };
    };

    Ok(StreamingCompletion {
        provider: provider_string,
        model: model_string,
        events: Box::pin(stream),
    })
}

fn parse_openai_stream_chunk(
    provider: &str,
    data: &str,
    final_usage: &mut Option<OpenAiUsage>,
) -> Result<Vec<String>> {
    let parsed: OpenAiStreamChunk =
        serde_json::from_str(data).with_context(|| format!("openai stream json: {data}"))?;
    if let Some(error) = parsed.error {
        let message = error
            .message
            .or(error.kind)
            .unwrap_or_else(|| "unknown upstream error".to_string());
        if let Some(capacity) = provider_stream_capacity_error(provider, &message, None) {
            return Err(capacity);
        }
        return Err(anyhow!("openai stream error: {message}"));
    }
    if let Some(usage) = parsed.usage {
        *final_usage = Some(usage);
    }
    Ok(parsed
        .choices
        .into_iter()
        .filter_map(|choice| choice.delta.content)
        .filter(|content| !content.is_empty())
        .collect())
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

// ─── Gemini ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct GeminiGenerateReq {
    contents: Vec<GeminiContent>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(rename = "generationConfig", skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Serialize)]
struct GeminiContent {
    role: &'static str,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    InlineData {
        #[serde(rename = "inlineData")]
        inline_data: GeminiInlineData,
    },
}

#[derive(Serialize)]
struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct GeminiGenerateResp {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsage>,
    error: Option<GeminiError>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiRespContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiRespContent {
    #[serde(default)]
    parts: Vec<GeminiRespPart>,
}

#[derive(Deserialize)]
struct GeminiRespPart {
    text: Option<String>,
}

#[derive(Clone, Deserialize)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<i64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<i64>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<i64>,
}

#[derive(Deserialize)]
struct GeminiError {
    message: Option<String>,
    status: Option<String>,
}

impl GeminiGenerateResp {
    fn text(self) -> String {
        self.candidates
            .into_iter()
            .filter_map(|candidate| candidate.content)
            .flat_map(|content| content.parts)
            .filter_map(|part| part.text)
            .collect::<Vec<_>>()
            .join("")
    }
}

impl GeminiUsage {
    fn token_counts(&self, fallback_input_tokens: i64) -> (i64, i64) {
        let input_tokens = self.prompt_token_count.unwrap_or(fallback_input_tokens);
        let output_tokens = self
            .candidates_token_count
            .or_else(|| {
                self.total_token_count
                    .zip(Some(input_tokens))
                    .map(|(total, input)| total.saturating_sub(input).max(0))
            })
            .unwrap_or(0);
        (input_tokens, output_tokens)
    }
}

#[allow(clippy::too_many_arguments)]
async fn gemini_complete(
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
    let req = gemini_generate_req(
        system,
        user,
        max_tokens,
        temperature,
        thinking,
        image_data_urls,
    )?;
    let resp = reqwest::Client::new()
        .post(gemini_url(model, false))
        .query(&[("key", key)])
        .json(&req)
        .send()
        .await
        .context("gemini http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("gemini", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }
    let parsed: GeminiGenerateResp = resp.json().await.context("gemini json")?;
    if let Some(error) = parsed.error {
        return Err(anyhow!(
            "gemini error: {}",
            error
                .message
                .or(error.status)
                .unwrap_or_else(|| "unknown upstream error".to_string())
        ));
    }
    let fallback_input = fallback_input_tokens.unwrap_or(0);
    let (input_tokens, output_tokens) = parsed
        .usage_metadata
        .as_ref()
        .map(|usage| usage.token_counts(fallback_input))
        .unwrap_or((fallback_input, 0));
    Ok(Completion {
        text: parsed.text(),
        provider: "gemini".to_string(),
        model: model.to_string(),
        input_tokens,
        output_tokens,
    })
}

#[allow(clippy::too_many_arguments)]
async fn gemini_complete_stream(
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<StreamingCompletion> {
    let req = gemini_generate_req(
        system,
        user,
        max_tokens,
        temperature,
        thinking,
        image_data_urls,
    )?;
    let resp = reqwest::Client::new()
        .post(gemini_url(model, true))
        .query(&[("key", key), ("alt", "sse")])
        .json(&req)
        .send()
        .await
        .context("gemini stream http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("gemini", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }

    let provider = "gemini".to_string();
    let model_string = model.to_string();
    let fallback_input = fallback_input_tokens.unwrap_or(0);
    let mut bytes = resp.bytes_stream();
    let stream = async_stream::try_stream! {
        let mut buffer = String::new();
        let mut pending_utf8 = Vec::new();
        let mut final_usage: Option<GeminiUsage> = None;
        let mut seen_done = false;
        let mut seen_terminal = false;

        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.context("gemini stream read")?;
            append_utf8_chunk(&chunk, &mut pending_utf8, &mut buffer)?;
            while let Some((_, data)) = take_sse_event(&mut buffer) {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                if data == "[DONE]" {
                    seen_done = true;
                    continue;
                }
                for delta in parse_gemini_stream_chunk(data, &mut final_usage, &mut seen_terminal)? {
                    yield CompletionStreamEvent::Delta(delta);
                }
            }
        }
        if !pending_utf8.is_empty() {
            let tail = std::str::from_utf8(&pending_utf8).context("gemini stream trailing utf8")?;
            buffer.push_str(tail);
        }
        while let Some((_, data)) = take_sse_event(&mut buffer) {
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                seen_done = true;
                continue;
            }
            for delta in parse_gemini_stream_chunk(data, &mut final_usage, &mut seen_terminal)? {
                yield CompletionStreamEvent::Delta(delta);
            }
        }
        if !seen_done && !seen_terminal {
            Err::<(), anyhow::Error>(anyhow!("gemini stream ended before terminal marker"))?;
        }
        if seen_done && final_usage.is_none() {
            Err::<(), anyhow::Error>(anyhow!("gemini stream ended before final usage"))?;
        }
        let usage = final_usage
            .ok_or_else(|| anyhow!("gemini stream ended before final usage"))?;
        let (input_tokens, output_tokens) = usage.token_counts(fallback_input);
        yield CompletionStreamEvent::Done {
            input_tokens,
            output_tokens,
        };
    };

    Ok(StreamingCompletion {
        provider,
        model: model_string,
        events: Box::pin(stream),
    })
}

#[allow(clippy::too_many_arguments)]
fn gemini_generate_req(
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    image_data_urls: &[String],
) -> Result<GeminiGenerateReq> {
    let mut parts = Vec::with_capacity(image_data_urls.len() + 1);
    parts.push(GeminiPart::Text {
        text: user.to_string(),
    });
    for data_url in image_data_urls {
        parts.push(GeminiPart::InlineData {
            inline_data: gemini_inline_data_from_data_url(data_url)?,
        });
    }
    Ok(GeminiGenerateReq {
        contents: vec![GeminiContent {
            role: "user",
            parts,
        }],
        system_instruction: if system.trim().is_empty() {
            None
        } else {
            Some(GeminiSystemInstruction {
                parts: vec![GeminiPart::Text {
                    text: system.to_string(),
                }],
            })
        },
        generation_config: Some(GeminiGenerationConfig {
            max_output_tokens: Some(effective_max_output_tokens(max_tokens, thinking)),
            temperature,
        }),
    })
}

fn gemini_inline_data_from_data_url(data_url: &str) -> Result<GeminiInlineData> {
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| anyhow!("gemini image payload must be a data URL"))?;
    let (metadata, data) = rest
        .split_once(',')
        .ok_or_else(|| anyhow!("gemini image data URL is missing payload"))?;
    let mut metadata_parts = metadata.split(';');
    let mime_type = metadata_parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("gemini image data URL is missing MIME type"))?;
    let is_base64 = metadata_parts.any(|part| part.eq_ignore_ascii_case("base64"));
    if !is_base64 {
        return Err(anyhow!("gemini image data URL must be base64 encoded"));
    }
    if !matches!(
        mime_type,
        "image/png" | "image/jpeg" | "image/webp" | "image/gif"
    ) {
        return Err(anyhow!("unsupported gemini image MIME type: {mime_type}"));
    }
    if data.trim().is_empty() {
        return Err(anyhow!("gemini image data URL has an empty payload"));
    }
    Ok(GeminiInlineData {
        mime_type: mime_type.to_string(),
        data: data.to_string(),
    })
}

fn gemini_url(model: &str, stream: bool) -> String {
    let method = if stream {
        "streamGenerateContent"
    } else {
        "generateContent"
    };
    override_url(
        &format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:{method}"),
        "BLUEY_TEST_GEMINI_URL",
    )
}

fn parse_gemini_stream_chunk(
    data: &str,
    final_usage: &mut Option<GeminiUsage>,
    seen_terminal: &mut bool,
) -> Result<Vec<String>> {
    let parsed: GeminiGenerateResp =
        serde_json::from_str(data).with_context(|| format!("gemini stream json: {data}"))?;
    if let Some(error) = parsed.error {
        let message = error
            .message
            .clone()
            .or(error.status.clone())
            .unwrap_or_else(|| "unknown upstream error".to_string());
        if let Some(capacity) =
            provider_stream_capacity_error("gemini", &message, error.status.as_deref())
        {
            return Err(capacity);
        }
        return Err(anyhow!("gemini stream error: {message}"));
    }
    if let Some(usage) = parsed.usage_metadata.clone() {
        *final_usage = Some(usage);
    }
    if parsed.candidates.iter().any(|candidate| {
        candidate
            .finish_reason
            .as_deref()
            .is_some_and(|reason| !reason.is_empty())
    }) {
        *seen_terminal = true;
    }
    Ok(parsed
        .candidates
        .into_iter()
        .filter_map(|candidate| candidate.content)
        .flat_map(|content| content.parts)
        .filter_map(|part| part.text)
        .filter(|text| !text.is_empty())
        .collect())
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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
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
            "image payloads are only supported by managed vision routes"
        ));
    }

    let thinking_req = anthropic_thinking_for(model, thinking);
    let effective_max_tokens = if thinking_req.is_some() {
        effective_max_output_tokens(max_tokens, thinking)
    } else {
        effective_max_output_tokens(max_tokens, ThinkingBudget::off())
    };
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
        stream: None,
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

#[derive(Deserialize)]
struct AnthropicStreamPayload {
    #[serde(rename = "type")]
    kind: String,
    delta: Option<AnthropicStreamDelta>,
    message: Option<AnthropicStreamMessage>,
    usage: Option<AnthropicStreamUsage>,
    error: Option<AnthropicStreamError>,
}

#[derive(Deserialize)]
struct AnthropicStreamError {
    #[serde(rename = "type")]
    kind: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicStreamMessage {
    usage: Option<AnthropicStreamUsage>,
}

#[derive(Deserialize)]
struct AnthropicStreamUsage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
}

#[allow(clippy::too_many_arguments)]
async fn anthropic_complete_stream(
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking: ThinkingBudget,
    fallback_input_tokens: Option<i64>,
    image_data_urls: &[String],
) -> Result<StreamingCompletion> {
    if !image_data_urls.is_empty() {
        return Err(anyhow!(
            "image payloads are only supported by managed vision routes"
        ));
    }

    let thinking_req = anthropic_thinking_for(model, thinking);
    let effective_max_tokens = if thinking_req.is_some() {
        effective_max_output_tokens(max_tokens, thinking)
    } else {
        effective_max_output_tokens(max_tokens, ThinkingBudget::off())
    };
    let req = AnthropicReq {
        model,
        max_tokens: effective_max_tokens,
        system,
        messages: vec![AnthropicMessage {
            role: "user",
            content: user,
        }],
        temperature: if thinking_req.is_some() {
            None
        } else {
            temperature
        },
        thinking: thinking_req,
        stream: Some(true),
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
        .context("anthropic stream http")?;
    let status = resp.status();
    if !status.is_success() {
        let error = upstream_http_error("anthropic", status, resp.headers());
        let _ = resp.text().await.unwrap_or_default();
        return Err(error);
    }

    let provider = "anthropic".to_string();
    let model_string = model.to_string();
    let stream_model = model_string.clone();
    let fallback_input = fallback_input_tokens.unwrap_or(0);
    let mut bytes = resp.bytes_stream();
    let stream = async_stream::try_stream! {
        let mut buffer = String::new();
        let mut pending_utf8 = Vec::new();
        let mut input_tokens: Option<i64> = None;
        let mut output_tokens: Option<i64> = None;
        let mut seen_stop = false;
        let mut output_chars: usize = 0;

        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.context("anthropic stream read")?;
            append_utf8_chunk(&chunk, &mut pending_utf8, &mut buffer)?;
            while let Some((event, data)) = take_sse_event(&mut buffer) {
                if let Some(delta) = parse_anthropic_stream_event(&event, &data, &mut input_tokens, &mut output_tokens, &mut seen_stop)? {
                    output_chars = output_chars.saturating_add(delta.chars().count());
                    yield CompletionStreamEvent::Delta(delta);
                }
            }
        }
        if !pending_utf8.is_empty() {
            let tail = std::str::from_utf8(&pending_utf8).context("anthropic stream trailing utf8")?;
            buffer.push_str(tail);
        }
        while let Some((event, data)) = take_sse_event(&mut buffer) {
            if let Some(delta) = parse_anthropic_stream_event(&event, &data, &mut input_tokens, &mut output_tokens, &mut seen_stop)? {
                output_chars = output_chars.saturating_add(delta.chars().count());
                yield CompletionStreamEvent::Delta(delta);
            }
        }
        let (input_tokens, output_tokens) = anthropic_stream_usage_or_estimate(
            &stream_model,
            seen_stop,
            input_tokens,
            output_tokens,
            output_chars,
            fallback_input,
        )?;
        yield CompletionStreamEvent::Done {
            input_tokens,
            output_tokens,
        };
    };

    Ok(StreamingCompletion {
        provider,
        model: model_string,
        events: Box::pin(stream),
    })
}

fn parse_anthropic_stream_event(
    event: &str,
    data: &str,
    input_tokens: &mut Option<i64>,
    output_tokens: &mut Option<i64>,
    seen_stop: &mut bool,
) -> Result<Option<String>> {
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }
    let parsed: AnthropicStreamPayload =
        serde_json::from_str(data).with_context(|| format!("anthropic stream json: {data}"))?;
    if event == "error" || parsed.kind == "error" {
        let (message, kind) = parsed
            .error
            .map(|error| {
                let kind = error.kind;
                let message = error
                    .message
                    .clone()
                    .or(kind.clone())
                    .unwrap_or_else(|| "unknown upstream error".to_string());
                (message, kind)
            })
            .unwrap_or_else(|| ("unknown upstream error".to_string(), None));
        if let Some(capacity) =
            provider_stream_capacity_error("anthropic", &message, kind.as_deref())
        {
            return Err(capacity);
        }
        return Err(anyhow!("anthropic stream error: {message}"));
    }
    if event == "message_start" || parsed.kind == "message_start" {
        if let Some(usage) = parsed.message.and_then(|message| message.usage) {
            if let Some(value) = usage.input_tokens {
                *input_tokens = Some(value);
            }
            if let Some(value) = usage.output_tokens {
                *output_tokens = Some(value);
            }
        }
    }
    if event == "message_delta" || parsed.kind == "message_delta" {
        if let Some(usage) = parsed.usage {
            if let Some(value) = usage.input_tokens.filter(|value| *value > 0) {
                *input_tokens = Some(value);
            }
            if let Some(value) = usage.output_tokens {
                *output_tokens = Some(value);
            }
        }
    }
    if event == "message_stop" || parsed.kind == "message_stop" {
        *seen_stop = true;
    }
    if event == "content_block_delta" || parsed.kind == "content_block_delta" {
        if let Some(delta) = parsed.delta {
            if delta.kind.as_deref() == Some("text_delta") || delta.text.is_some() {
                return Ok(delta.text.filter(|text| !text.is_empty()));
            }
        }
    }
    Ok(None)
}

fn provider_stream_capacity_error(
    provider: &str,
    message: &str,
    kind: Option<&str>,
) -> Option<anyhow::Error> {
    let mut lower = message.to_ascii_lowercase();
    if let Some(kind) = kind {
        lower.push(' ');
        lower.push_str(&kind.to_ascii_lowercase());
    }
    let is_capacity = lower.contains("rate_limit")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("resource_exhausted")
        || lower.contains("overload")
        || lower.contains("capacity")
        || lower.contains("quota");
    if !is_capacity {
        return None;
    }
    let status = if provider == "anthropic" && lower.contains("overload") {
        529
    } else {
        429
    };
    Some(anyhow!(UpstreamHttpError {
        provider: provider.to_string(),
        status,
        retry_after_secs: Some(default_capacity_cooldown_secs()),
    }))
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
    let _ = model;
    // Current Anthropic flagship models reject the older
    // `thinking.type=enabled` request shape. Keep manual thinking off until
    // Bluey implements the newer adaptive/output_config effort schema.
    false
}

fn append_utf8_chunk(bytes: &[u8], pending: &mut Vec<u8>, buffer: &mut String) -> Result<()> {
    pending.extend_from_slice(bytes);
    loop {
        match std::str::from_utf8(pending) {
            Ok(valid) => {
                buffer.push_str(valid);
                pending.clear();
                return Ok(());
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&pending[..valid_up_to])
                        .expect("valid_up_to marks valid utf8");
                    buffer.push_str(valid);
                    pending.drain(..valid_up_to);
                    continue;
                }
                if err.error_len().is_some() {
                    return Err(anyhow!("invalid stream utf8: {err}"));
                }
                return Ok(());
            }
        }
    }
}

fn take_sse_event(buffer: &mut String) -> Option<(String, String)> {
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
    let mut event = "message".to_string();
    let mut data = Vec::new();
    for line in frame.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(value) = line.strip_prefix("event:") {
            event = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string());
        }
    }
    Some((event, data.join("\n")))
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

pub async fn embed_batch_with_key(
    api_key: &str,
    provider: &str,
    model: &str,
    inputs: &[String],
) -> Result<EmbedBatchCompletion> {
    match provider {
        "openai" => openai_embed_batch(api_key, model, inputs).await,
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

#[derive(Debug, Clone)]
pub struct EmbedBatchCompletion {
    pub vectors: Vec<Vec<f32>>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
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
    let batch = openai_embed_batch(key, model, &[input.to_string()]).await?;
    let vector = batch
        .vectors
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("openai embed: no data returned"))?;
    Ok(EmbedCompletion {
        vector,
        provider: batch.provider,
        model: batch.model,
        input_tokens: batch.input_tokens,
    })
}

async fn openai_embed_batch(
    key: &str,
    model: &str,
    inputs: &[String],
) -> Result<EmbedBatchCompletion> {
    if inputs.is_empty() {
        return Err(anyhow!("openai embed: no input returned"));
    }
    let req = serde_json::json!({
        "model": model,
        "input": inputs,
    });
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
    if parsed.data.len() != inputs.len() {
        return Err(anyhow!(
            "openai embed: expected {} vectors, got {}",
            inputs.len(),
            parsed.data.len()
        ));
    }
    let vectors = parsed
        .data
        .into_iter()
        .map(|item| item.embedding)
        .collect::<Vec<_>>();
    let input_tokens = parsed
        .usage
        .map(|u| u.prompt_tokens)
        .unwrap_or_else(|| inputs.iter().map(|input| (input.len() as i64) / 4).sum());
    Ok(EmbedBatchCompletion {
        vectors,
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
        assert_eq!(resolve_route("instant"), ("openai", "gpt-5.4-mini"));
        assert_eq!(
            resolve_route("balanced"),
            ("anthropic", "claude-sonnet-4-6"),
        );
        assert_eq!(resolve_route("deep"), ("anthropic", "claude-opus-4-8"),);
        assert_eq!(
            resolve_route("vision"),
            ("gemini", "gemini-3-flash-preview")
        );
        assert_eq!(resolve_route("local"), ("unsupported", "local"));
        // Unknown → balanced default.
        assert_eq!(resolve_route("???"), ("anthropic", "claude-sonnet-4-6"),);
    }

    #[test]
    fn route_candidates_preserve_2026_lane_order() {
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("instant", RoutePolicy::QualityFirst, ""),
            vec![
                ("openai", "gpt-5.4-mini"),
                ("deepseek", "deepseek-v4-flash"),
                ("gemini", "gemini-3.1-flash-lite"),
                ("anthropic", "claude-haiku-4-5-20251001"),
                ("gemini", "gemini-3-flash-preview"),
                ("anthropic", "claude-sonnet-4-6")
            ]
        );
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("balanced", RoutePolicy::QualityFirst, ""),
            vec![
                ("anthropic", "claude-sonnet-4-6"),
                ("deepseek", "deepseek-v4-flash"),
                ("zai", "glm-5.2"),
                ("gemini", "gemini-3.1-pro-preview"),
                ("openai", "gpt-5.5"),
                ("gemini", "gemini-3-flash-preview"),
                ("openai", "gpt-5.4-mini")
            ]
        );
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("deep", RoutePolicy::QualityFirst, ""),
            vec![
                ("anthropic", "claude-opus-4-8"),
                ("zai", "glm-5.2"),
                ("deepseek", "deepseek-v4-pro"),
                ("gemini", "gemini-3.1-pro-preview"),
                ("openai", "gpt-5.5"),
                ("anthropic", "claude-sonnet-4-6"),
                ("deepseek", "deepseek-v4-flash"),
                ("gemini", "gemini-3-flash-preview")
            ]
        );
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("vision", RoutePolicy::QualityFirst, ""),
            vec![
                ("openai", "gpt-5.5"),
                ("gemini", "gemini-3.1-pro-preview"),
                ("gemini", "gemini-3-flash-preview"),
                ("openai", "gpt-5.4-mini")
            ]
        );
        assert!(
            resolve_route_candidates_for_policy_and_seed("local", RoutePolicy::QualityFirst, "")
                .is_empty(),
            "managed cloud must not dispatch daemon-only local lanes"
        );
    }

    #[test]
    fn cost_optimized_policy_prefers_glm_and_deepseek_text_routes() {
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("instant", RoutePolicy::CostOptimized, ""),
            vec![
                ("deepseek", "deepseek-v4-flash"),
                ("gemini", "gemini-3.1-flash-lite"),
                ("openai", "gpt-5.4-mini"),
                ("anthropic", "claude-haiku-4-5-20251001"),
                ("gemini", "gemini-3-flash-preview"),
                ("anthropic", "claude-sonnet-4-6")
            ]
        );
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed(
                "balanced",
                RoutePolicy::CostOptimized,
                ""
            ),
            vec![
                ("zai", "glm-5.2"),
                ("deepseek", "deepseek-v4-flash"),
                ("anthropic", "claude-sonnet-4-6"),
                ("gemini", "gemini-3.1-pro-preview"),
                ("openai", "gpt-5.5"),
                ("gemini", "gemini-3-flash-preview"),
                ("openai", "gpt-5.4-mini")
            ]
        );
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("deep", RoutePolicy::CostOptimized, ""),
            vec![
                ("zai", "glm-5.2"),
                ("deepseek", "deepseek-v4-pro"),
                ("anthropic", "claude-opus-4-8"),
                ("gemini", "gemini-3.1-pro-preview"),
                ("openai", "gpt-5.5"),
                ("anthropic", "claude-sonnet-4-6"),
                ("deepseek", "deepseek-v4-flash"),
                ("gemini", "gemini-3-flash-preview")
            ]
        );
        assert_eq!(
            resolve_route_candidates_for_policy_and_seed("vision", RoutePolicy::CostOptimized, ""),
            resolve_route_candidates_for_policy_and_seed("vision", RoutePolicy::QualityFirst, ""),
            "GLM/DeepSeek text policy must not steal image routes"
        );
    }

    #[test]
    fn provider_mix_rotates_first_text_provider_by_seed() {
        for lane in ["instant", "balanced", "deep"] {
            let mut first_providers = Vec::new();
            for idx in 0..80 {
                let seed = format!("mix-request-{idx}");
                let Some((provider, _model)) = resolve_route_candidates_for_policy_and_seed(
                    lane,
                    RoutePolicy::ProviderMix,
                    &seed,
                )
                .into_iter()
                .next() else {
                    panic!("provider mix returned no candidates for {lane}");
                };
                if !first_providers.contains(&provider) {
                    first_providers.push(provider);
                }
            }

            for provider in ["anthropic", "deepseek", "gemini", "openai", "zai"] {
                assert!(
                    first_providers.contains(&provider),
                    "provider mix should rotate {lane} first attempts across {provider}; got {first_providers:?}"
                );
            }
        }
    }

    #[test]
    fn provider_mix_keeps_vision_on_image_capable_routes() {
        for idx in 0..24 {
            let seed = format!("vision-request-{idx}");
            let routes = resolve_route_candidates_for_policy_and_seed(
                "vision",
                RoutePolicy::ProviderMix,
                &seed,
            );
            assert!(
                routes
                    .iter()
                    .all(|(provider, _model)| *provider == "openai" || *provider == "gemini"),
                "vision provider mix must not route image payloads to text-only providers: {routes:?}"
            );
            assert!(
                !matches!(routes.first(), Some(("gemini", GEMINI_PRO_MODEL))),
                "vision provider mix should keep the slower pro preview as fallback, not first: {routes:?}"
            );
        }
    }

    #[test]
    fn route_candidates_have_pricing_entries() {
        for policy in [
            RoutePolicy::ProviderMix,
            RoutePolicy::QualityFirst,
            RoutePolicy::CostOptimized,
        ] {
            for lane in ["instant", "balanced", "deep", "vision"] {
                for (provider, model) in
                    resolve_route_candidates_for_policy_and_seed(lane, policy, "")
                {
                    assert!(
                        crate::pricing::lookup(provider, model).is_some(),
                        "missing pricing for {policy:?} {lane} candidate {provider}/{model}"
                    );
                }
            }
        }
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

    #[test]
    fn gemini_generate_req_serializes_text_and_image_parts() {
        let images = vec!["data:image/png;base64,aGVsbG8=".to_string()];
        let req = gemini_generate_req(
            "answer clearly",
            "What is on screen?",
            Some(1024),
            Some(0.2),
            ThinkingBudget::off(),
            &images,
        )
        .unwrap();
        let value = serde_json::to_value(req).unwrap();

        assert_eq!(
            value["systemInstruction"]["parts"][0]["text"],
            "answer clearly"
        );
        assert_eq!(value["contents"][0]["role"], "user");
        assert_eq!(
            value["contents"][0]["parts"][0]["text"],
            "What is on screen?"
        );
        assert_eq!(
            value["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 1024);
    }

    #[test]
    fn gemini_stream_error_frame_is_not_treated_as_empty_success() {
        let mut usage = None;
        let mut seen_terminal = false;
        let err = parse_gemini_stream_chunk(
            r#"{"error":{"message":"provider overloaded","status":"RESOURCE_EXHAUSTED"}}"#,
            &mut usage,
            &mut seen_terminal,
        )
        .unwrap_err();

        let capacity = err.downcast_ref::<UpstreamHttpError>().unwrap();
        assert_eq!(capacity.provider, "gemini");
        assert_eq!(capacity.status, 429);
        assert!(capacity.retry_after_secs.is_some());
        assert!(usage.is_none());
        assert!(!seen_terminal);
    }

    #[test]
    fn gemini_stream_chunk_tracks_final_usage() {
        let mut usage = None;
        let mut seen_terminal = false;
        let deltas = parse_gemini_stream_chunk(
            r#"{"candidates":[{"content":{"parts":[{"text":"hello"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":7,"candidatesTokenCount":3,"totalTokenCount":10}}"#,
            &mut usage,
            &mut seen_terminal,
        )
        .unwrap();

        assert_eq!(deltas, vec!["hello"]);
        assert_eq!(usage.unwrap().token_counts(0), (7, 3));
        assert!(seen_terminal);
    }

    #[test]
    fn openai_stream_error_frame_is_not_treated_as_empty_success() {
        let mut usage = None;
        let err = parse_openai_stream_chunk(
            "openai",
            r#"{"error":{"message":"provider overloaded","type":"rate_limit_error"}}"#,
            &mut usage,
        )
        .unwrap_err();

        let capacity = err.downcast_ref::<UpstreamHttpError>().unwrap();
        assert_eq!(capacity.provider, "openai");
        assert_eq!(capacity.status, 429);
        assert!(capacity.retry_after_secs.is_some());
        assert!(usage.is_none());
    }

    #[test]
    fn openai_stream_without_done_after_text_is_error() {
        match openai_stream_usage_or_estimate("zai", "glm-5.2", false, None, 18, Some(12)) {
            Ok(_) => panic!("expected missing DONE error"),
            Err(err) => assert!(err.to_string().contains("before [DONE]")),
        }
    }

    #[test]
    fn openai_stream_without_done_and_without_text_is_error() {
        match openai_stream_usage_or_estimate("zai", "glm-5.2", false, None, 0, Some(12)) {
            Ok(_) => panic!("expected missing DONE error"),
            Err(err) => assert!(err.to_string().contains("before [DONE]")),
        }
    }

    #[test]
    fn anthropic_stream_error_event_is_not_treated_as_empty_success() {
        let mut input_tokens = None;
        let mut output_tokens = None;
        let mut seen_stop = false;
        let err = parse_anthropic_stream_event(
            "error",
            r#"{"type":"error","error":{"type":"overloaded_error","message":"provider busy"}}"#,
            &mut input_tokens,
            &mut output_tokens,
            &mut seen_stop,
        )
        .unwrap_err();

        let capacity = err.downcast_ref::<UpstreamHttpError>().unwrap();
        assert_eq!(capacity.provider, "anthropic");
        assert_eq!(capacity.status, 529);
        assert!(capacity.retry_after_secs.is_some());
        assert!(input_tokens.is_none());
        assert!(output_tokens.is_none());
        assert!(!seen_stop);
    }

    #[test]
    fn anthropic_stream_without_stop_after_text_is_error() {
        let err =
            anthropic_stream_usage_or_estimate("claude-sonnet-4-6", false, Some(9), None, 21, 7)
                .unwrap_err();

        assert!(err.to_string().contains("message_stop"));
    }

    #[test]
    fn anthropic_stream_without_stop_and_without_text_is_error() {
        let err =
            anthropic_stream_usage_or_estimate("claude-sonnet-4-6", false, Some(9), None, 0, 7)
                .unwrap_err();

        assert!(err.to_string().contains("message_stop"));
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_date() {
        assert_eq!(parse_retry_after_value("12"), Some(12));

        let future = (Utc::now() + chrono::Duration::seconds(30))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let parsed = parse_retry_after_value(&future).unwrap();
        assert!((1..=30).contains(&parsed));
    }

    #[tokio::test]
    async fn anthropic_rejects_image_payloads() {
        let images = vec!["data:image/png;base64,aGVsbG8=".to_string()];
        let err = anthropic_complete(
            "sk-test",
            "claude-sonnet-4-6",
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
            .contains("image payloads are only supported by managed vision routes"));
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
    fn non_thinking_output_is_clamped_to_ceiling() {
        std::env::remove_var("BLUEY_MAX_OUTPUT_TOKENS");
        let off = ThinkingBudget::off();
        // A large non-thinking request is clamped to the default ceiling.
        assert_eq!(
            effective_max_output_tokens(Some(8000), off),
            DEFAULT_MAX_OUTPUT_TOKENS
        );
        // A small request is honored as-is.
        assert_eq!(effective_max_output_tokens(Some(256), off), 256);
        // Default (None) stays at the existing 2048 baseline.
        assert_eq!(effective_max_output_tokens(None, off), 2048);
        // Env override raises the ceiling.
        std::env::set_var("BLUEY_MAX_OUTPUT_TOKENS", "4096");
        assert_eq!(effective_max_output_tokens(Some(8000), off), 4096);
        std::env::remove_var("BLUEY_MAX_OUTPUT_TOKENS");
    }

    #[test]
    fn openai_limit_fields_use_effective_output_budget() {
        let off = ThinkingBudget::off();
        let expected = effective_max_output_tokens(Some(8000), off);

        let (max_tokens, max_completion_tokens) =
            openai_effective_token_limit_fields("gpt-5.4-mini", Some(8000), off);
        assert_eq!(max_tokens, None);
        assert_eq!(max_completion_tokens, Some(expected));

        let (max_tokens, max_completion_tokens) =
            openai_effective_token_limit_fields("gpt-4o", Some(8000), off);
        assert_eq!(max_tokens, Some(expected));
        assert_eq!(max_completion_tokens, None);
    }

    #[test]
    fn openai_thinking_lanes_keep_larger_output_budget() {
        std::env::remove_var("BLUEY_MAX_OUTPUT_TOKENS");
        let budget = resolve_thinking_budget("deep", None, None);

        let (max_tokens, max_completion_tokens) =
            openai_effective_token_limit_fields("gpt-5.5", None, budget);
        assert_eq!(max_tokens, None);
        assert_eq!(max_completion_tokens, Some(5120));
    }

    #[test]
    fn openai_compatible_thinking_is_provider_scoped() {
        let (thinking, effort) = openai_compatible_thinking_for("openai", ThinkingBudget::off());
        assert!(thinking.is_none());
        assert!(effort.is_none());

        let (thinking, effort) = openai_compatible_thinking_for("deepseek", ThinkingBudget::off());
        assert_eq!(thinking.unwrap().ty, "disabled");
        assert_eq!(effort, None);

        let budget = ThinkingBudget {
            mode: ThinkingMode::High,
            max_tokens: Some(4096),
        };
        let (thinking, effort) = openai_compatible_thinking_for("zai", budget);
        assert_eq!(thinking.unwrap().ty, "enabled");
        assert_eq!(effort, Some("max"));
    }

    #[test]
    fn openai_gpt5_temperature_is_omitted() {
        assert_eq!(
            openai_compatible_temperature_for("openai", "gpt-5.5", Some(0.1)),
            None
        );
        assert_eq!(
            openai_compatible_temperature_for("openai", "gpt-5.4-mini", Some(0.2)),
            None
        );
        assert_eq!(
            openai_compatible_temperature_for("openai", "gpt-4o", Some(0.2)),
            Some(0.2)
        );
        assert_eq!(
            openai_compatible_temperature_for("zai", "glm-5.2", Some(0.2)),
            Some(0.2)
        );
    }

    #[test]
    fn anthropic_manual_thinking_disabled_until_adaptive_schema() {
        let budget = resolve_thinking_budget("deep", None, None);
        assert!(anthropic_thinking_for("claude-opus-4-8", budget).is_none());
        assert!(anthropic_thinking_for("claude-sonnet-4-6", budget).is_none());
        assert!(anthropic_thinking_for("claude-haiku-4-5-20251001", budget).is_none());
        assert!(anthropic_thinking_for("claude-fable-5-20260609", budget).is_none());
        assert!(anthropic_thinking_for("gpt-5.5", budget).is_none());
    }
}
