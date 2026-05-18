//! Data model for the auto router.

use serde::{Deserialize, Serialize};

/// What kind of task the user is asking Bluey to help with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    /// Catch-all: small talk, factual lookup, definitions.
    General,
    /// Implementation, debugging, code review.
    Code,
    /// Architecture, trade-offs, large design questions.
    SystemDesign,
    /// Meeting transcript / decision / action-item retrieval.
    Meeting,
    /// Drafting prose, summaries, emails.
    Writing,
    /// Image / screenshot understanding.
    Vision,
}

/// How hard the question is. Drives the deep/balanced lane choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    /// One-liner, lookup, paraphrase. Use the cheapest model.
    Easy,
    /// Default complexity. Balanced provider.
    Medium,
    /// Multi-step reasoning, design, complex code. Strong model + larger budget.
    Hard,
}

/// Which context surfaces the question wants to pull in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ContextNeeds {
    /// Recent meeting transcript (system + user lines).
    pub transcript: bool,
    /// Active web/document page text the dashboard or overlay captured.
    pub page: bool,
    /// User-attached files.
    pub files: bool,
    /// A screenshot artifact.
    pub screenshot: bool,
    /// RAG memory (prior cards, decisions, etc.).
    pub memory: bool,
}

impl ContextNeeds {
    /// Helper: any vision-style context needed.
    pub fn needs_vision(&self) -> bool {
        self.screenshot
    }

    /// Helper: did the user attach anything?
    pub fn needs_attachments(&self) -> bool {
        self.files || self.screenshot
    }
}

/// Latency budget for the response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatencyLane {
    /// "Right now" — small/cheap streaming model. Optimised for first-token latency.
    Instant,
    /// Default. Strong model with normal token budget.
    Balanced,
    /// "Take your time" — strongest model, larger token budget, may be non-streaming.
    Deep,
}

/// Concrete routing decision: a target provider + parameter overrides.
///
/// `provider_name` is matched against `cue_llm::LlmProvider::name()` at
/// dispatch time; the dispatcher selects the first provider whose name matches
/// or falls back to the default `LlmRouter` round-robin if no match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRoute {
    /// Symbolic lane name (instant/balanced/deep/vision/local).
    pub lane: ProviderLane,
    /// Preferred provider name, e.g. `"openai"`, `"anthropic"`, `"ollama"`.
    pub provider_name: String,
    /// Preferred model, e.g. `"gpt-4o-mini"`. Empty = provider default.
    pub model: String,
    /// Token budget override; None = provider default.
    pub max_tokens: Option<u32>,
    /// Temperature override; None = provider default.
    pub temperature: Option<f32>,
    /// Whether streaming is preferred for this lane.
    pub stream: bool,
}

/// Symbolic lane name. Stable across policy changes; the concrete provider /
/// model bound to a lane is configured in `RoutingPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderLane {
    /// Fast cheap streaming.
    Instant,
    /// Default quality.
    Balanced,
    /// Strong, larger budget.
    Deep,
    /// Vision-capable provider.
    Vision,
    /// Local-only (Ollama or similar). Privacy / offline fallback.
    Local,
}

/// Output of a `TaskClassifier::classify` call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskClassification {
    /// Task taxonomy bucket.
    pub task_type: TaskType,
    /// Difficulty estimate.
    pub difficulty: Difficulty,
    /// Context surfaces the classifier thinks the prompt depends on.
    pub needed_context: ContextNeeds,
    /// Recommended latency lane.
    pub latency_lane: LatencyLane,
    /// Confidence in the classification, in [0.0, 1.0].
    /// 1.0 = clear unambiguous match; <0.5 = poor signal, callers may want
    /// to escalate to a tiny-model classifier.
    pub confidence: f32,
}
