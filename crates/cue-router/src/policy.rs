//! Routing policy: turns a `TaskClassification` into a concrete `ProviderRoute`.
//!
//! `RoutingPolicy` is a trait so different deployments can plug in their own
//! lane→provider mapping (managed Bluey routing service vs. self-hosted vs.
//! local-only).
//!
//! `StaticPolicy` is the default v0.1 implementation: a hard-coded mapping
//! from `LatencyLane` + `TaskType` to a fixed provider name + model + token
//! budget. Callers can override individual lanes when constructing the policy.

use crate::model::{LatencyLane, ProviderLane, ProviderRoute, TaskClassification, TaskType};

/// Policy trait. Implementations may be sync because the lookup is normally
/// cheap; if a future managed implementation needs to do an RPC, it can do so
/// behind a cache.
pub trait RoutingPolicy: Send + Sync {
    /// Resolve a classification to a concrete `ProviderRoute`.
    fn route(&self, classification: &TaskClassification) -> ProviderRoute;
}

/// A fixed lane → provider mapping. The default values aim at the providers
/// already wired in `cue-llm` (`anthropic`, `openai`, `ollama`).
#[derive(Debug, Clone)]
pub struct StaticPolicy {
    /// Provider name + model for the Instant lane.
    pub instant: ProviderRoute,
    /// Provider name + model for the Balanced lane.
    pub balanced: ProviderRoute,
    /// Provider name + model for the Deep lane.
    pub deep: ProviderRoute,
    /// Provider name + model for the Vision lane.
    pub vision: ProviderRoute,
    /// Provider name + model for the Local-only lane.
    pub local: ProviderRoute,
    /// If true, the user opted into local-only and `route()` always returns
    /// `local` regardless of classification.
    pub force_local: bool,
}

impl StaticPolicy {
    /// Default policy. Each lane points at a sensible provider/model. Callers
    /// override fields as needed (e.g. swap Anthropic for OpenAI on the deep
    /// lane).
    pub fn defaults() -> Self {
        Self {
            instant: ProviderRoute {
                lane: ProviderLane::Instant,
                provider_name: "openai".to_string(),
                model: "gpt-4o-mini".to_string(),
                max_tokens: Some(512),
                temperature: Some(0.3),
                stream: true,
            },
            balanced: ProviderRoute {
                lane: ProviderLane::Balanced,
                provider_name: "anthropic".to_string(),
                model: "claude-3-5-sonnet-latest".to_string(),
                max_tokens: Some(2048),
                temperature: Some(0.3),
                stream: true,
            },
            deep: ProviderRoute {
                lane: ProviderLane::Deep,
                provider_name: "anthropic".to_string(),
                model: "claude-3-7-sonnet-latest".to_string(),
                max_tokens: Some(8192),
                temperature: Some(0.2),
                stream: false,
            },
            vision: ProviderRoute {
                lane: ProviderLane::Vision,
                provider_name: "openai".to_string(),
                model: "gpt-4o".to_string(),
                max_tokens: Some(2048),
                temperature: Some(0.2),
                stream: true,
            },
            local: ProviderRoute {
                lane: ProviderLane::Local,
                provider_name: "ollama".to_string(),
                model: "llama3.1".to_string(),
                max_tokens: Some(2048),
                temperature: Some(0.3),
                stream: true,
            },
            force_local: false,
        }
    }

    /// Return a policy that always routes to the local lane regardless of
    /// classification (privacy / offline mode).
    pub fn local_only() -> Self {
        Self {
            force_local: true,
            ..Self::defaults()
        }
    }
}

impl Default for StaticPolicy {
    fn default() -> Self {
        Self::defaults()
    }
}

impl RoutingPolicy for StaticPolicy {
    fn route(&self, classification: &TaskClassification) -> ProviderRoute {
        if self.force_local {
            return self.local.clone();
        }
        // Vision ALWAYS goes to the vision lane regardless of latency hint —
        // the other lanes may not be vision-capable.
        if classification.task_type == TaskType::Vision {
            return self.vision.clone();
        }
        match classification.latency_lane {
            LatencyLane::Instant => self.instant.clone(),
            LatencyLane::Balanced => self.balanced.clone(),
            LatencyLane::Deep => self.deep.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContextNeeds, Difficulty};

    fn make_classification(task_type: TaskType, lane: LatencyLane) -> TaskClassification {
        TaskClassification {
            task_type,
            difficulty: Difficulty::Medium,
            needed_context: ContextNeeds::default(),
            latency_lane: lane,
            confidence: 0.8,
        }
    }

    #[test]
    fn instant_lane_routes_to_instant_provider() {
        let p = StaticPolicy::defaults();
        let r = p.route(&make_classification(TaskType::General, LatencyLane::Instant));
        assert_eq!(r.lane, ProviderLane::Instant);
        assert!(r.stream);
    }

    #[test]
    fn deep_lane_routes_to_deep_provider() {
        let p = StaticPolicy::defaults();
        let r = p.route(&make_classification(TaskType::Code, LatencyLane::Deep));
        assert_eq!(r.lane, ProviderLane::Deep);
        assert!(r.max_tokens.unwrap_or(0) >= 4096);
    }

    #[test]
    fn vision_overrides_lane() {
        let p = StaticPolicy::defaults();
        let r = p.route(&make_classification(TaskType::Vision, LatencyLane::Instant));
        assert_eq!(r.lane, ProviderLane::Vision);
    }

    #[test]
    fn force_local_overrides_everything() {
        let p = StaticPolicy::local_only();
        let r = p.route(&make_classification(TaskType::Vision, LatencyLane::Deep));
        assert_eq!(r.lane, ProviderLane::Local);
    }
}
