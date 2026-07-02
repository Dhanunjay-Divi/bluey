use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::clock;

pub const DEFAULT_LATENCY_TARGET_MS: u64 = 2_500;
pub const DEFAULT_LATENCY_TIMEOUT_MS: u64 = 15_000;
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1_024;
pub const DEFAULT_MAX_TOTAL_TOKENS: u32 = 16_384;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AiProviderId(String);

impl AiProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Self::default()
        } else {
            Self(trimmed.to_string())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AiProviderId {
    fn default() -> Self {
        Self("default".to_string())
    }
}

impl From<&str> for AiProviderId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for AiProviderId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for AiProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AiModelId(String);

impl AiModelId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let trimmed = value.trim();
        Self(trimmed.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AiModelId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for AiModelId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for AiModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    CueManaged,
    #[serde(rename = "openai")]
    OpenAi,
    Anthropic,
    Google,
    #[serde(rename = "azure_openai")]
    AzureOpenAi,
    Mistral,
    Groq,
    Cerebras,
    Cohere,
    Deepgram,
    Local,
    Custom,
}

impl Default for AiProviderKind {
    fn default() -> Self {
        Self::Custom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiCapability {
    Chat,
    Vision,
    Stt,
    Embeddings,
}

impl std::fmt::Display for AiCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chat => write!(f, "chat"),
            Self::Vision => write!(f, "vision"),
            Self::Stt => write!(f, "stt"),
            Self::Embeddings => write!(f, "embeddings"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiCapabilities {
    pub chat: bool,
    pub vision: bool,
    pub stt: bool,
    pub embeddings: bool,
}

impl AiCapabilities {
    pub fn none() -> Self {
        Self {
            chat: false,
            vision: false,
            stt: false,
            embeddings: false,
        }
    }

    pub fn chat() -> Self {
        Self::none().with_chat()
    }

    pub fn all() -> Self {
        Self {
            chat: true,
            vision: true,
            stt: true,
            embeddings: true,
        }
    }

    pub fn from_capabilities(capabilities: impl IntoIterator<Item = AiCapability>) -> Self {
        capabilities
            .into_iter()
            .fold(Self::none(), |caps, capability| caps.with(capability))
    }

    pub fn with(mut self, capability: AiCapability) -> Self {
        match capability {
            AiCapability::Chat => self.chat = true,
            AiCapability::Vision => self.vision = true,
            AiCapability::Stt => self.stt = true,
            AiCapability::Embeddings => self.embeddings = true,
        }
        self
    }

    pub fn with_chat(self) -> Self {
        self.with(AiCapability::Chat)
    }

    pub fn with_vision(self) -> Self {
        self.with(AiCapability::Vision)
    }

    pub fn with_stt(self) -> Self {
        self.with(AiCapability::Stt)
    }

    pub fn with_embeddings(self) -> Self {
        self.with(AiCapability::Embeddings)
    }

    pub fn supports(&self, capability: AiCapability) -> bool {
        match capability {
            AiCapability::Chat => self.chat,
            AiCapability::Vision => self.vision,
            AiCapability::Stt => self.stt,
            AiCapability::Embeddings => self.embeddings,
        }
    }

    pub fn supports_all<I, C>(&self, capabilities: I) -> bool
    where
        I: IntoIterator<Item = C>,
        C: std::borrow::Borrow<AiCapability>,
    {
        capabilities
            .into_iter()
            .all(|capability| self.supports(*capability.borrow()))
    }
}

impl Default for AiCapabilities {
    fn default() -> Self {
        Self::chat()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyBudget {
    pub target_ms: u64,
    pub timeout_ms: u64,
}

impl LatencyBudget {
    pub fn new(target_ms: u64, timeout_ms: u64) -> Self {
        Self {
            target_ms,
            timeout_ms: timeout_ms.max(target_ms),
        }
    }

    pub fn realtime() -> Self {
        Self::new(750, 4_000)
    }

    pub fn interactive() -> Self {
        Self::default()
    }

    pub fn background() -> Self {
        Self::new(10_000, 60_000)
    }
}

impl Default for LatencyBudget {
    fn default() -> Self {
        Self::new(DEFAULT_LATENCY_TARGET_MS, DEFAULT_LATENCY_TIMEOUT_MS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CostBudget {
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub max_total_tokens: Option<u32>,
    pub max_usd: Option<f64>,
}

impl CostBudget {
    pub fn new(
        max_input_tokens: Option<u32>,
        max_output_tokens: Option<u32>,
        max_total_tokens: Option<u32>,
        max_usd: Option<f64>,
    ) -> Self {
        Self {
            max_input_tokens,
            max_output_tokens,
            max_total_tokens,
            max_usd,
        }
    }

    pub fn bounded(max_usd: f64) -> Self {
        Self {
            max_usd: Some(max_usd),
            ..Self::default()
        }
    }

    pub fn unbounded() -> Self {
        Self {
            max_input_tokens: None,
            max_output_tokens: None,
            max_total_tokens: None,
            max_usd: None,
        }
    }
}

impl Default for CostBudget {
    fn default() -> Self {
        Self {
            max_input_tokens: None,
            max_output_tokens: Some(DEFAULT_MAX_OUTPUT_TOKENS),
            max_total_tokens: Some(DEFAULT_MAX_TOTAL_TOKENS),
            max_usd: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RouteBudget {
    pub latency: LatencyBudget,
    pub cost: CostBudget,
}

impl RouteBudget {
    pub fn new(latency: LatencyBudget, cost: CostBudget) -> Self {
        Self { latency, cost }
    }

    pub fn realtime() -> Self {
        Self::new(LatencyBudget::realtime(), CostBudget::default())
    }

    pub fn background() -> Self {
        Self::new(LatencyBudget::background(), CostBudget::default())
    }
}

impl Default for RouteBudget {
    fn default() -> Self {
        Self::new(LatencyBudget::default(), CostBudget::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteSelectionPolicy {
    OrderedFallback,
    LowestLatency,
    LowestCost,
    Balanced,
}

impl Default for RouteSelectionPolicy {
    fn default() -> Self {
        Self::OrderedFallback
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSelector {
    pub provider_id: AiProviderId,
    pub provider_kind: AiProviderKind,
    pub model: Option<AiModelId>,
}

impl ProviderSelector {
    pub fn new(provider_id: impl Into<AiProviderId>, provider_kind: AiProviderKind) -> Self {
        Self {
            provider_id: provider_id.into(),
            provider_kind,
            model: None,
        }
    }

    pub fn openai(model: impl Into<AiModelId>) -> Self {
        Self::new("openai", AiProviderKind::OpenAi).with_model(model)
    }

    pub fn cue_managed(model: impl Into<AiModelId>) -> Self {
        Self::new("bluey_managed", AiProviderKind::CueManaged).with_model(model)
    }

    pub fn anthropic(model: impl Into<AiModelId>) -> Self {
        Self::new("anthropic", AiProviderKind::Anthropic).with_model(model)
    }

    pub fn groq(model: impl Into<AiModelId>) -> Self {
        Self::new("groq", AiProviderKind::Groq).with_model(model)
    }

    pub fn cerebras(model: impl Into<AiModelId>) -> Self {
        Self::new("cerebras", AiProviderKind::Cerebras).with_model(model)
    }

    pub fn deepgram(model: impl Into<AiModelId>) -> Self {
        Self::new("deepgram", AiProviderKind::Deepgram).with_model(model)
    }

    pub fn google(model: impl Into<AiModelId>) -> Self {
        Self::new("google", AiProviderKind::Google).with_model(model)
    }

    pub fn local(model: impl Into<AiModelId>) -> Self {
        Self::new("local", AiProviderKind::Local).with_model(model)
    }

    pub fn with_model(mut self, model: impl Into<AiModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn model_or<'a>(&'a self, default_model: &'a str) -> &'a str {
        self.model
            .as_ref()
            .map(AiModelId::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(default_model)
    }

    pub fn display_label(&self) -> String {
        self.model
            .as_ref()
            .map(|model| format!("{}/{}", self.provider_id, model))
            .unwrap_or_else(|| self.provider_id.to_string())
    }
}

impl Default for ProviderSelector {
    fn default() -> Self {
        Self::new(AiProviderId::default(), AiProviderKind::default())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRouteStep {
    pub provider: ProviderSelector,
    #[serde(default)]
    pub required_capabilities: Vec<AiCapability>,
    pub budget_override: Option<RouteBudget>,
}

impl ProviderRouteStep {
    pub fn new(provider: ProviderSelector) -> Self {
        Self {
            provider,
            required_capabilities: Vec::new(),
            budget_override: None,
        }
    }

    pub fn require(mut self, capability: AiCapability) -> Self {
        push_unique(&mut self.required_capabilities, capability);
        self
    }

    pub fn with_budget(mut self, budget: RouteBudget) -> Self {
        self.budget_override = Some(budget);
        self
    }
}

impl From<ProviderSelector> for ProviderRouteStep {
    fn from(provider: ProviderSelector) -> Self {
        Self::new(provider)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRoute {
    pub primary: ProviderRouteStep,
    #[serde(default)]
    pub fallbacks: Vec<ProviderRouteStep>,
    #[serde(default)]
    pub required_capabilities: Vec<AiCapability>,
    #[serde(default)]
    pub budgets: RouteBudget,
    #[serde(default)]
    pub policy: RouteSelectionPolicy,
    #[serde(default)]
    pub safety: SafetyFlags,
    #[serde(default)]
    pub privacy: PrivacyFlags,
}

impl ProviderRoute {
    pub fn direct(provider: ProviderSelector) -> Self {
        Self {
            primary: ProviderRouteStep::new(provider).require(AiCapability::Chat),
            fallbacks: Vec::new(),
            required_capabilities: vec![AiCapability::Chat],
            budgets: RouteBudget::default(),
            policy: RouteSelectionPolicy::default(),
            safety: SafetyFlags::default(),
            privacy: PrivacyFlags::default(),
        }
    }

    pub fn managed_commercial() -> Self {
        Self::direct(ProviderSelector::cue_managed("bluey-router-v1"))
            .require(AiCapability::Vision)
            .require(AiCapability::Stt)
            .require(AiCapability::Embeddings)
            .with_fallback(ProviderSelector::cerebras("llama3.1-8b"))
            .with_fallback(ProviderSelector::groq("llama-3.1-8b-instant"))
            .with_fallback(ProviderSelector::openai("gpt-4.1-mini"))
            .with_fallback(ProviderSelector::local("bluey-local-answer-v0"))
            .with_budgets(RouteBudget::realtime())
            .with_policy(RouteSelectionPolicy::Balanced)
            .with_privacy(PrivacyFlags::managed_commercial())
    }

    pub fn with_fallback(mut self, fallback: impl Into<ProviderRouteStep>) -> Self {
        self.fallbacks.push(fallback.into());
        self
    }

    pub fn require(mut self, capability: AiCapability) -> Self {
        push_unique(&mut self.required_capabilities, capability);
        self.primary = self.primary.require(capability);
        self
    }

    pub fn with_budgets(mut self, budgets: RouteBudget) -> Self {
        self.budgets = budgets;
        self
    }

    pub fn with_policy(mut self, policy: RouteSelectionPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_safety(mut self, safety: SafetyFlags) -> Self {
        self.safety = safety;
        self
    }

    pub fn with_privacy(mut self, privacy: PrivacyFlags) -> Self {
        self.privacy = privacy;
        self
    }

    pub fn steps(&self) -> impl Iterator<Item = &ProviderRouteStep> {
        std::iter::once(&self.primary).chain(self.fallbacks.iter())
    }
}

impl Default for ProviderRoute {
    fn default() -> Self {
        Self::managed_commercial()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyFlags {
    pub moderate_input: bool,
    pub moderate_output: bool,
    pub block_prompt_injection: bool,
    pub require_grounded_answers: bool,
    pub allow_sensitive_advice: bool,
    pub allow_unsafe_code: bool,
}

impl SafetyFlags {
    pub fn strict() -> Self {
        Self {
            moderate_input: true,
            moderate_output: true,
            block_prompt_injection: true,
            require_grounded_answers: true,
            allow_sensitive_advice: false,
            allow_unsafe_code: false,
        }
    }

    pub fn permissive() -> Self {
        Self {
            moderate_input: false,
            moderate_output: false,
            block_prompt_injection: false,
            require_grounded_answers: false,
            allow_sensitive_advice: true,
            allow_unsafe_code: true,
        }
    }
}

impl Default for SafetyFlags {
    fn default() -> Self {
        Self {
            moderate_input: true,
            moderate_output: true,
            block_prompt_injection: true,
            require_grounded_answers: false,
            allow_sensitive_advice: false,
            allow_unsafe_code: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyFlags {
    pub allow_cloud_processing: bool,
    pub allow_pii: bool,
    pub redact_pii: bool,
    pub allow_audio_upload: bool,
    pub allow_image_upload: bool,
    pub allow_provider_training: bool,
    pub retain_provider_logs: bool,
}

impl PrivacyFlags {
    pub fn managed_commercial() -> Self {
        Self {
            allow_cloud_processing: true,
            allow_pii: false,
            redact_pii: true,
            allow_audio_upload: false,
            allow_image_upload: false,
            allow_provider_training: false,
            retain_provider_logs: false,
        }
    }

    pub fn local_only() -> Self {
        Self {
            allow_cloud_processing: false,
            allow_pii: false,
            redact_pii: true,
            allow_audio_upload: false,
            allow_image_upload: false,
            allow_provider_training: false,
            retain_provider_logs: false,
        }
    }

    pub fn with_pii(mut self) -> Self {
        self.allow_pii = true;
        self.redact_pii = false;
        self
    }

    pub fn with_audio_upload(mut self) -> Self {
        self.allow_audio_upload = true;
        self
    }

    pub fn with_image_upload(mut self) -> Self {
        self.allow_image_upload = true;
        self
    }

    pub fn permits_capability(&self, capability: AiCapability) -> bool {
        self.permits_cloud_capability(capability)
    }

    pub fn permits_cloud_capability(&self, capability: AiCapability) -> bool {
        match capability {
            AiCapability::Chat | AiCapability::Embeddings => self.allow_cloud_processing,
            AiCapability::Vision => self.allow_cloud_processing && self.allow_image_upload,
            AiCapability::Stt => self.allow_cloud_processing && self.allow_audio_upload,
        }
    }
}

impl Default for PrivacyFlags {
    fn default() -> Self {
        Self::managed_commercial()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerContextKind {
    Transcript,
    MeetingMemory,
    Screenshot,
    Document,
    UserNote,
    System,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl Default for DataSensitivity {
    fn default() -> Self {
        Self::Internal
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerContext {
    pub kind: AnswerContextKind,
    pub content: String,
    pub title: Option<String>,
    pub source: Option<String>,
    #[serde(default)]
    pub sensitivity: DataSensitivity,
}

impl AnswerContext {
    pub fn new(kind: AnswerContextKind, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
            title: None,
            source: None,
            sensitivity: DataSensitivity::default(),
        }
    }

    pub fn transcript(content: impl Into<String>) -> Self {
        Self::new(AnswerContextKind::Transcript, content)
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: DataSensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerRequestMetadata {
    pub request_id: Uuid,
    pub created_at: String,
    pub meeting_id: Option<Uuid>,
    pub correlation_id: Option<String>,
    pub stream: bool,
    #[serde(default)]
    pub required_capabilities: Vec<AiCapability>,
    #[serde(default)]
    pub visible_context_ids: Vec<Uuid>,
}

impl AnswerRequestMetadata {
    pub fn new() -> Self {
        Self {
            request_id: Uuid::new_v4(),
            created_at: clock::now_epoch_ms_string(),
            meeting_id: None,
            correlation_id: None,
            stream: false,
            required_capabilities: vec![AiCapability::Chat],
            visible_context_ids: Vec::new(),
        }
    }

    pub fn with_meeting_id(mut self, meeting_id: Uuid) -> Self {
        self.meeting_id = Some(meeting_id);
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    pub fn require(mut self, capability: AiCapability) -> Self {
        push_unique(&mut self.required_capabilities, capability);
        self
    }

    pub fn with_visible_context_ids(mut self, ids: Vec<Uuid>) -> Self {
        self.visible_context_ids = ids;
        self
    }
}

impl Default for AnswerRequestMetadata {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerRequest {
    pub metadata: AnswerRequestMetadata,
    pub question: String,
    pub instructions: Option<String>,
    #[serde(default)]
    pub context: Vec<AnswerContext>,
    pub route: ProviderRoute,
}

impl AnswerRequest {
    pub fn new(question: impl Into<String>, route: ProviderRoute) -> Self {
        let mut metadata = AnswerRequestMetadata::new();
        for capability in route.required_capabilities.iter().copied() {
            metadata = metadata.require(capability);
        }

        Self {
            metadata,
            question: question.into(),
            instructions: None,
            context: Vec::new(),
            route,
        }
    }

    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    pub fn with_context(mut self, context: AnswerContext) -> Self {
        self.context.push(context);
        self
    }

    pub fn streaming(mut self) -> Self {
        self.metadata = self.metadata.streaming();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderClientConfig {
    pub provider: ProviderSelector,
    pub capabilities: AiCapabilities,
    pub endpoint: Option<String>,
    pub api_key_env: Option<String>,
    pub api_key_configured: bool,
    pub live_requests_enabled: bool,
}

impl ProviderClientConfig {
    pub fn new(provider: ProviderSelector, capabilities: AiCapabilities) -> Self {
        Self {
            provider,
            capabilities,
            endpoint: None,
            api_key_env: None,
            api_key_configured: false,
            live_requests_enabled: false,
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        let endpoint = endpoint.into();
        if !endpoint.trim().is_empty() {
            self.endpoint = Some(endpoint);
        }
        self
    }

    pub fn with_api_key_env(mut self, env_var: impl Into<String>, configured: bool) -> Self {
        let env_var = env_var.into();
        if !env_var.trim().is_empty() {
            self.api_key_env = Some(env_var);
        }
        self.api_key_configured = configured;
        self
    }

    pub fn with_live_requests_enabled(mut self, enabled: bool) -> Self {
        self.live_requests_enabled = enabled;
        self
    }

    pub fn missing_configuration_message(&self) -> Option<String> {
        if matches!(self.provider.provider_kind, AiProviderKind::Local) {
            return None;
        }

        match (&self.api_key_env, self.api_key_configured, &self.endpoint) {
            (Some(env_var), false, _) => Some(format!("missing provider credential: {env_var}")),
            (None, _, _) => Some("missing provider credential configuration".to_string()),
            (_, true, None) => Some("missing provider endpoint".to_string()),
            _ => None,
        }
    }

    pub fn unavailable_message(&self) -> Option<String> {
        self.missing_configuration_message().or_else(|| {
            if self.live_requests_enabled
                || matches!(self.provider.provider_kind, AiProviderKind::Local)
            {
                None
            } else {
                Some(
                    "provider credentials are configured, but this build has no HTTP adapter wired"
                        .to_string(),
                )
            }
        })
    }

    pub fn can_attempt_live_request(&self) -> bool {
        self.unavailable_message().is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRequestPayload {
    pub request_id: Uuid,
    pub provider: ProviderSelector,
    pub endpoint: Option<String>,
    pub model: String,
    pub question: String,
    pub instructions: Option<String>,
    #[serde(default)]
    pub context: Vec<AnswerContext>,
    pub stream: bool,
    #[serde(default)]
    pub required_capabilities: Vec<AiCapability>,
    pub safety: SafetyFlags,
    pub privacy: PrivacyFlags,
    pub latency_timeout_ms: u64,
    pub max_output_tokens: Option<u32>,
}

impl ProviderRequestPayload {
    pub fn from_request(
        request: &AnswerRequest,
        provider: ProviderSelector,
        endpoint: Option<String>,
        default_model: &str,
        budget: RouteBudget,
    ) -> Self {
        Self {
            request_id: request.metadata.request_id,
            model: provider.model_or(default_model).to_string(),
            provider,
            endpoint,
            question: request.question.clone(),
            instructions: request.instructions.clone(),
            context: request.context.clone(),
            stream: request.metadata.stream,
            required_capabilities: request.metadata.required_capabilities.clone(),
            safety: request.route.safety,
            privacy: request.route.privacy,
            latency_timeout_ms: budget.latency.timeout_ms,
            max_output_tokens: budget.cost.max_output_tokens,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteAttemptStatus {
    Pending,
    Succeeded,
    Failed,
    TimedOut,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteAttemptMetadata {
    pub provider: ProviderSelector,
    pub status: RouteAttemptStatus,
    pub fallback_depth: usize,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

impl RouteAttemptMetadata {
    pub fn started(provider: ProviderSelector, fallback_depth: usize) -> Self {
        Self {
            provider,
            status: RouteAttemptStatus::Pending,
            fallback_depth,
            started_at: clock::now_epoch_ms_string(),
            ended_at: None,
            latency_ms: None,
            error: None,
        }
    }

    pub fn succeeded(mut self, latency_ms: u64) -> Self {
        self.status = RouteAttemptStatus::Succeeded;
        self.ended_at = Some(clock::now_epoch_ms_string());
        self.latency_ms = Some(latency_ms);
        self.error = None;
        self
    }

    pub fn failed(mut self, message: impl Into<String>) -> Self {
        self.status = RouteAttemptStatus::Failed;
        self.ended_at = Some(clock::now_epoch_ms_string());
        self.error = Some(message.into());
        self
    }

    pub fn timed_out(mut self) -> Self {
        self.status = RouteAttemptStatus::TimedOut;
        self.ended_at = Some(clock::now_epoch_ms_string());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerFinishReason {
    Stop,
    Length,
    Safety,
    ProviderError,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    pub fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.saturating_add(output_tokens),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    pub amount: f64,
    pub currency: String,
    pub estimated: bool,
}

impl CostEstimate {
    pub fn usd(amount: f64) -> Self {
        Self {
            amount,
            currency: "USD".to_string(),
            estimated: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyOutcome {
    pub input_filtered: bool,
    pub output_filtered: bool,
    #[serde(default)]
    pub notices: Vec<String>,
}

impl SafetyOutcome {
    pub fn pass() -> Self {
        Self::default()
    }

    pub fn blocked(notice: impl Into<String>) -> Self {
        Self {
            input_filtered: true,
            output_filtered: true,
            notices: vec![notice.into()],
        }
    }

    pub fn with_notice(mut self, notice: impl Into<String>) -> Self {
        self.notices.push(notice.into());
        self
    }
}

impl Default for SafetyOutcome {
    fn default() -> Self {
        Self {
            input_filtered: false,
            output_filtered: false,
            notices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerResponseMetadata {
    pub request_id: Uuid,
    pub response_id: Uuid,
    pub created_at: String,
    pub provider: ProviderSelector,
    #[serde(default)]
    pub requested_route: Option<ProviderRoute>,
    #[serde(default)]
    pub attempts: Vec<RouteAttemptMetadata>,
    pub latency_ms: Option<u64>,
    pub token_usage: Option<TokenUsage>,
    pub cost_estimate: Option<CostEstimate>,
    pub finish_reason: Option<AnswerFinishReason>,
    #[serde(default)]
    pub safety: SafetyOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<AnswerSourceMetadata>,
}

impl AnswerResponseMetadata {
    pub fn new(request_id: Uuid, provider: ProviderSelector) -> Self {
        Self {
            request_id,
            response_id: Uuid::new_v4(),
            created_at: clock::now_epoch_ms_string(),
            provider,
            requested_route: None,
            attempts: Vec::new(),
            latency_ms: None,
            token_usage: None,
            cost_estimate: None,
            finish_reason: None,
            safety: SafetyOutcome::default(),
            sources: Vec::new(),
        }
    }

    pub fn with_requested_route(mut self, route: ProviderRoute) -> Self {
        self.requested_route = Some(route);
        self
    }

    pub fn with_attempt(mut self, attempt: RouteAttemptMetadata) -> Self {
        self.attempts.push(attempt);
        self
    }

    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.latency_ms = Some(latency_ms);
        self
    }

    pub fn with_usage(mut self, usage: TokenUsage) -> Self {
        self.token_usage = Some(usage);
        self
    }

    pub fn with_cost(mut self, cost: CostEstimate) -> Self {
        self.cost_estimate = Some(cost);
        self
    }

    pub fn finished(mut self, reason: AnswerFinishReason) -> Self {
        self.finish_reason = Some(reason);
        self
    }

    pub fn with_safety(mut self, safety: SafetyOutcome) -> Self {
        self.safety = safety;
        self
    }

    pub fn with_sources(mut self, sources: Vec<AnswerSourceMetadata>) -> Self {
        self.sources = sources;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSourceMetadata {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerRetrievalStatus {
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerResponse {
    pub metadata: AnswerResponseMetadata,
    pub answer: String,
}

impl AnswerResponse {
    pub fn new(request_id: Uuid, provider: ProviderSelector, answer: impl Into<String>) -> Self {
        Self {
            metadata: AnswerResponseMetadata::new(request_id, provider),
            answer: answer.into(),
        }
    }

    pub fn with_metadata(mut self, metadata: AnswerResponseMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnswerStreamEvent {
    Started {
        request_id: Uuid,
        provider: ProviderSelector,
    },
    Delta {
        request_id: Uuid,
        text: String,
    },
    RetrievalStatus {
        request_id: Uuid,
        status: AnswerRetrievalStatus,
    },
    Sources {
        request_id: Uuid,
        sources: Vec<AnswerSourceMetadata>,
    },
    ProviderSwitch {
        request_id: Uuid,
        attempt: RouteAttemptMetadata,
    },
    SafetyNotice {
        request_id: Uuid,
        safety: SafetyOutcome,
    },
    Usage {
        request_id: Uuid,
        usage: TokenUsage,
        cost_estimate: Option<CostEstimate>,
    },
    Completed {
        response: AnswerResponse,
    },
    Error {
        request_id: Option<Uuid>,
        provider: Option<ProviderSelector>,
        message: String,
        retryable: bool,
    },
}

impl AnswerStreamEvent {
    pub fn started(request_id: Uuid, provider: ProviderSelector) -> Self {
        Self::Started {
            request_id,
            provider,
        }
    }

    pub fn delta(request_id: Uuid, text: impl Into<String>) -> Self {
        Self::Delta {
            request_id,
            text: text.into(),
        }
    }

    pub fn retrieval_status(
        request_id: Uuid,
        stage: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::RetrievalStatus {
            request_id,
            status: AnswerRetrievalStatus {
                stage: stage.into(),
                message: message.into(),
            },
        }
    }

    pub fn sources(request_id: Uuid, sources: Vec<AnswerSourceMetadata>) -> Self {
        Self::Sources {
            request_id,
            sources,
        }
    }

    pub fn completed(response: AnswerResponse) -> Self {
        Self::Completed { response }
    }

    pub fn error(request_id: Option<Uuid>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Error {
            request_id,
            provider: None,
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Unknown,
    Healthy,
    Degraded,
    Unavailable,
    Disabled,
}

impl Default for ProviderHealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: ProviderSelector,
    #[serde(default)]
    pub capabilities: AiCapabilities,
    #[serde(default)]
    pub health: ProviderHealthStatus,
    pub checked_at: Option<String>,
    pub average_latency_ms: Option<u64>,
    pub consecutive_failures: u32,
    pub message: Option<String>,
}

impl ProviderStatus {
    pub fn unknown(provider: ProviderSelector) -> Self {
        Self {
            provider,
            capabilities: AiCapabilities::default(),
            health: ProviderHealthStatus::Unknown,
            checked_at: None,
            average_latency_ms: None,
            consecutive_failures: 0,
            message: None,
        }
    }

    pub fn healthy(provider: ProviderSelector, capabilities: AiCapabilities) -> Self {
        Self {
            provider,
            capabilities,
            health: ProviderHealthStatus::Healthy,
            checked_at: Some(clock::now_epoch_ms_string()),
            average_latency_ms: None,
            consecutive_failures: 0,
            message: None,
        }
    }

    pub fn unavailable(mut self, message: impl Into<String>) -> Self {
        self.health = ProviderHealthStatus::Unavailable;
        self.checked_at = Some(clock::now_epoch_ms_string());
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.message = Some(message.into());
        self
    }

    pub fn disabled(mut self, message: impl Into<String>) -> Self {
        self.health = ProviderHealthStatus::Disabled;
        self.checked_at = Some(clock::now_epoch_ms_string());
        self.message = Some(message.into());
        self
    }

    pub fn degraded(mut self, message: impl Into<String>) -> Self {
        self.health = ProviderHealthStatus::Degraded;
        self.checked_at = Some(clock::now_epoch_ms_string());
        self.message = Some(message.into());
        self
    }

    pub fn with_average_latency(mut self, average_latency_ms: u64) -> Self {
        self.average_latency_ms = Some(average_latency_ms);
        self
    }

    pub fn supports(&self, capability: AiCapability) -> bool {
        self.capabilities.supports(capability)
    }

    pub fn is_usable(&self) -> bool {
        matches!(
            self.health,
            ProviderHealthStatus::Healthy | ProviderHealthStatus::Degraded
        )
    }
}

impl Default for ProviderStatus {
    fn default() -> Self {
        Self::unknown(ProviderSelector::default())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRuntimeStatus {
    pub route: ProviderRoute,
    pub providers: Vec<ProviderStatus>,
    pub streaming_answers_enabled: bool,
    pub vision_enabled: bool,
    pub stt_enabled: bool,
    pub managed_cloud_required: bool,
    pub updated_at: String,
}

impl AiRuntimeStatus {
    pub fn scaffolded(providers: Vec<ProviderStatus>) -> Self {
        Self {
            route: ProviderRoute::managed_commercial(),
            providers,
            streaming_answers_enabled: false,
            vision_enabled: false,
            stt_enabled: false,
            managed_cloud_required: true,
            updated_at: clock::now_epoch_ms_string(),
        }
    }
}

fn push_unique<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_report_supported_modes() {
        let caps = AiCapabilities::chat().with_vision().with_embeddings();

        assert!(caps.supports(AiCapability::Chat));
        assert!(caps.supports(AiCapability::Vision));
        assert!(caps.supports(AiCapability::Embeddings));
        assert!(!caps.supports(AiCapability::Stt));
        assert!(caps.supports_all([AiCapability::Chat, AiCapability::Vision].iter()));
        assert!(!caps.supports_all([AiCapability::Chat, AiCapability::Stt].iter()));
    }

    #[test]
    fn route_builder_keeps_primary_fallbacks_and_defaults() {
        let route = ProviderRoute::direct(ProviderSelector::openai("gpt-4.1"))
            .require(AiCapability::Vision)
            .with_fallback(ProviderSelector::anthropic("claude-3-7-sonnet-latest"))
            .with_budgets(RouteBudget::realtime());

        assert_eq!(route.primary.provider.provider_id.as_str(), "openai");
        assert_eq!(route.fallbacks.len(), 1);
        assert_eq!(
            route.required_capabilities,
            vec![AiCapability::Chat, AiCapability::Vision]
        );
        assert_eq!(
            route.primary.required_capabilities,
            vec![AiCapability::Chat, AiCapability::Vision]
        );
        assert_eq!(route.budgets.latency.target_ms, 750);
        assert!(route.privacy.allow_cloud_processing);
        assert!(!route.privacy.allow_provider_training);
        assert_eq!(route.steps().count(), 2);
    }

    #[test]
    fn managed_route_has_fast_fallbacks_and_core_capabilities() {
        let route = ProviderRoute::managed_commercial();

        assert_eq!(
            route.primary.provider.provider_kind,
            AiProviderKind::CueManaged
        );
        assert_eq!(route.fallbacks.len(), 4);
        assert!(route
            .fallbacks
            .iter()
            .any(|step| step.provider.provider_kind == AiProviderKind::Local));
        assert!(!route.fallbacks.iter().any(|step| step
            .provider
            .model
            .as_ref()
            .is_some_and(|model| model.as_str().starts_with("managed-"))));
        assert!(route.required_capabilities.contains(&AiCapability::Chat));
        assert!(route.required_capabilities.contains(&AiCapability::Vision));
        assert!(route.required_capabilities.contains(&AiCapability::Stt));
        assert!(route
            .required_capabilities
            .contains(&AiCapability::Embeddings));
        assert!(route.privacy.allow_cloud_processing);
        assert!(!route.privacy.allow_audio_upload);
        assert!(!route.privacy.allow_image_upload);
    }

    #[test]
    fn request_response_and_stream_events_serialize() {
        let route = ProviderRoute::direct(ProviderSelector::openai("gpt-4.1-mini"));
        let request = AnswerRequest::new("What changed?", route)
            .with_context(AnswerContext::transcript(
                "Alice: We moved the launch date.",
            ))
            .streaming();

        let request_json = serde_json::to_string(&request).expect("serialize request");
        let round_trip: AnswerRequest =
            serde_json::from_str(&request_json).expect("deserialize request");
        assert_eq!(round_trip.question, "What changed?");
        assert!(round_trip.metadata.stream);

        let event = AnswerStreamEvent::delta(request.metadata.request_id, "The launch moved.");
        let event_json = serde_json::to_string(&event).expect("serialize event");

        assert!(event_json.contains(r#""type":"delta""#));
        assert!(event_json.contains("The launch moved."));

        let status = AnswerStreamEvent::retrieval_status(
            request.metadata.request_id,
            "checking_memory",
            "Checking conversation context",
        );
        let status_json = serde_json::to_string(&status).expect("serialize status event");
        assert!(status_json.contains(r#""type":"retrieval_status""#));
        assert!(status_json.contains("Checking conversation context"));
    }

    #[test]
    fn provider_status_helpers_track_health() {
        let provider = ProviderSelector::anthropic("claude-3-7-sonnet-latest");
        let healthy = ProviderStatus::healthy(provider.clone(), AiCapabilities::all())
            .with_average_latency(1_200);

        assert!(healthy.is_usable());
        assert!(healthy.supports(AiCapability::Stt));
        assert_eq!(healthy.average_latency_ms, Some(1_200));

        let disabled = ProviderStatus::unknown(provider).disabled("missing API key");
        assert!(!disabled.is_usable());
        assert_eq!(disabled.health, ProviderHealthStatus::Disabled);
    }

    #[test]
    fn provider_client_config_reports_missing_key_without_secret_values() {
        let config = ProviderClientConfig::new(
            ProviderSelector::openai("gpt-4.1-mini"),
            AiCapabilities::all(),
        )
        .with_endpoint("https://api.openai.com/v1/responses")
        .with_api_key_env("OPENAI_API_KEY", false);

        assert_eq!(
            config.unavailable_message(),
            Some("missing provider credential: OPENAI_API_KEY".to_string())
        );
        assert!(!config.can_attempt_live_request());
    }

    #[test]
    fn provider_payload_preserves_route_metadata_for_adapters() {
        let route = ProviderRoute::direct(ProviderSelector::openai("gpt-4.1-mini"));
        let request = AnswerRequest::new("What did we decide?", route)
            .with_instructions("Be brief")
            .with_context(AnswerContext::transcript("Alice: Ship it."))
            .streaming();

        let payload = ProviderRequestPayload::from_request(
            &request,
            ProviderSelector::openai("gpt-4.1-mini"),
            Some("https://api.openai.com/v1/responses".to_string()),
            "fallback-model",
            RouteBudget::realtime(),
        );

        assert_eq!(payload.request_id, request.metadata.request_id);
        assert_eq!(payload.model, "gpt-4.1-mini");
        assert_eq!(payload.context.len(), 1);
        assert!(payload.stream);
        assert_eq!(payload.latency_timeout_ms, 4_000);
        assert_eq!(payload.max_output_tokens, Some(DEFAULT_MAX_OUTPUT_TOKENS));
    }

    #[test]
    fn latency_budget_never_times_out_before_target() {
        let budget = LatencyBudget::new(5_000, 1_000);

        assert_eq!(budget.target_ms, 5_000);
        assert_eq!(budget.timeout_ms, 5_000);
    }
}
