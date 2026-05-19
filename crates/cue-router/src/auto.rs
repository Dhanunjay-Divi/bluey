//! High-level coordinator: classify + apply policy + honor `local_only`.
//!
//! This is the API most callers should use. It owns a classifier and a policy
//! and exposes a single `route(input, options)` that returns a `ProviderRoute`
//! ready to dispatch.
//!
//! Why this exists: `ClassifierInput` deliberately does NOT carry a
//! `local_only` flag because the classifier does not change its output based on
//! it (privacy is a policy concern, not a classification one). But callers
//! still want a one-call path that says "classify this prompt, and if I am in
//! local-only mode, route to the Local lane regardless." `AutoRouter::route`
//! is that one-call path.

use std::sync::Arc;

use crate::classifier::{ClassifierInput, TaskClassifier};
use crate::model::ProviderRoute;
use crate::policy::{RoutingPolicy, StaticPolicy};
use crate::TaskClassification;

/// Per-call routing options.
#[derive(Debug, Clone, Copy, Default)]
pub struct RouteOptions {
    /// If true, the returned `ProviderRoute` is forced to the Local lane
    /// regardless of classification. The classifier still runs (and its
    /// output is returned alongside the route) so callers can still observe
    /// task type, difficulty, and context needs.
    pub local_only: bool,
}

/// Output of an `AutoRouter::route` call.
#[derive(Debug, Clone)]
pub struct RoutedRequest {
    /// The classification produced by the classifier.
    pub classification: TaskClassification,
    /// The concrete provider route the caller should dispatch.
    pub route: ProviderRoute,
}

/// High-level coordinator: classify + apply policy.
pub struct AutoRouter {
    classifier: Arc<dyn TaskClassifier>,
    policy: Arc<dyn RoutingPolicy>,
    /// Local lane snapshot, used when `RouteOptions::local_only` is set.
    /// Captured once at construction so we do not need a separate
    /// `RoutingPolicy::local_route()` accessor.
    local_route: ProviderRoute,
}

impl AutoRouter {
    /// Construct from any classifier + policy. The local lane snapshot is
    /// taken from `StaticPolicy::defaults().local`; if you have a custom
    /// `local_route`, use [`AutoRouter::with_local_route`] instead.
    pub fn new(classifier: Arc<dyn TaskClassifier>, policy: Arc<dyn RoutingPolicy>) -> Self {
        Self::with_local_route(classifier, policy, StaticPolicy::defaults().local)
    }

    /// Construct with an explicit local-route override.
    pub fn with_local_route(
        classifier: Arc<dyn TaskClassifier>,
        policy: Arc<dyn RoutingPolicy>,
        local_route: ProviderRoute,
    ) -> Self {
        Self {
            classifier,
            policy,
            local_route,
        }
    }

    /// Classify the input and resolve to a concrete `ProviderRoute`.
    /// Honors `options.local_only` by short-circuiting to the local lane.
    pub async fn route(&self, input: &ClassifierInput<'_>, options: RouteOptions) -> RoutedRequest {
        let classification = self.classifier.classify(input).await;
        let route = if options.local_only {
            self.local_route.clone()
        } else {
            self.policy.route(&classification)
        };
        RoutedRequest {
            classification,
            route,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heuristic::HeuristicClassifier;
    use crate::model::ProviderLane;
    use crate::policy::StaticPolicy;

    fn make_router() -> AutoRouter {
        let classifier: Arc<dyn TaskClassifier> = Arc::new(HeuristicClassifier::new());
        let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
        AutoRouter::new(classifier, policy)
    }

    fn make_input(prompt: &str) -> ClassifierInput<'_> {
        ClassifierInput {
            prompt,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn auto_router_routes_easy_to_instant() {
        let r = make_router();
        let out = r
            .route(&make_input("what year is it"), RouteOptions::default())
            .await;
        assert_eq!(out.route.lane, ProviderLane::Instant);
    }

    #[tokio::test]
    async fn auto_router_routes_design_to_deep() {
        let r = make_router();
        let out = r
            .route(
                &make_input(
                    "design a system to handle 10k qps writes with global consistency \
                     and high availability; discuss tradeoffs between sharding strategies",
                ),
                RouteOptions::default(),
            )
            .await;
        assert_eq!(out.route.lane, ProviderLane::Deep);
    }

    #[tokio::test]
    async fn auto_router_local_only_forces_local_lane() {
        // The user explicitly opts into privacy / offline mode. Even a deep
        // design question must route to Local.
        let r = make_router();
        let out = r
            .route(
                &make_input("design a system to handle 10k qps writes with global consistency"),
                RouteOptions { local_only: true },
            )
            .await;
        assert_eq!(out.route.lane, ProviderLane::Local);
        // Classification still runs and remains accurate.
        assert_eq!(out.classification.task_type, crate::TaskType::SystemDesign);
    }

    #[tokio::test]
    async fn auto_router_vision_attachment_overrides_local_only_off() {
        // With local_only off, vision attachment routes to vision lane.
        let r = make_router();
        let mut input = make_input("write a haiku");
        input.has_screenshot = true;
        let out = r.route(&input, RouteOptions::default()).await;
        assert_eq!(out.route.lane, ProviderLane::Vision);
    }

    #[tokio::test]
    async fn auto_router_local_only_beats_vision() {
        // Privacy mode wins over vision routing.
        let r = make_router();
        let mut input = make_input("describe this");
        input.has_screenshot = true;
        let out = r.route(&input, RouteOptions { local_only: true }).await;
        assert_eq!(out.route.lane, ProviderLane::Local);
    }
}
