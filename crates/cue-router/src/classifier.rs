//! Classifier trait + layered (heuristic-then-model) implementation.

use async_trait::async_trait;

use crate::model::TaskClassification;

/// What the classifier sees about a single user request.
///
/// `attachments` covers files / screenshots / page captures. The exact content
/// is irrelevant to the heuristic classifier; only the count and kind matter
/// for routing.
///
/// **Note:** `local_only` is intentionally NOT a field here — it is a routing
/// policy concern, not a classification concern. Callers wanting privacy /
/// offline routing should use `AutoRouter::route(&input, RouteOptions {
/// local_only: true })` or construct a `StaticPolicy::local_only()` directly.
#[derive(Debug, Clone, Default)]
pub struct ClassifierInput<'a> {
    /// The user's natural-language prompt.
    pub prompt: &'a str,
    /// Whether a meeting transcript is currently in scope (active session).
    pub has_transcript: bool,
    /// Whether the dashboard/overlay has a captured page in scope.
    pub has_page: bool,
    /// Number of file attachments on the request.
    pub file_attachment_count: usize,
    /// Whether a screenshot artifact is on the request.
    pub has_screenshot: bool,
}

/// Trait every classifier (heuristic, tiny-model, future managed router) implements.
#[async_trait]
pub trait TaskClassifier: Send + Sync {
    /// Inspect the input and produce a classification with confidence.
    async fn classify(&self, input: &ClassifierInput<'_>) -> TaskClassification;
}

/// Two-stage classifier: run the heuristic first, escalate to the model only
/// when the heuristic's confidence drops below `escalation_threshold`.
///
/// In v0.1 the model classifier is optional (the field can be `None`). When it
/// is `None`, the heuristic result is returned as-is regardless of confidence.
pub struct LayeredClassifier {
    heuristic: Box<dyn TaskClassifier>,
    model: Option<Box<dyn TaskClassifier>>,
    escalation_threshold: f32,
}

impl LayeredClassifier {
    /// Build a layered classifier. `escalation_threshold` is in [0.0, 1.0].
    /// If the heuristic returns `confidence < escalation_threshold` AND a model
    /// classifier is configured, the model is consulted and its result is
    /// returned.
    pub fn new(
        heuristic: Box<dyn TaskClassifier>,
        model: Option<Box<dyn TaskClassifier>>,
        escalation_threshold: f32,
    ) -> Self {
        Self {
            heuristic,
            model,
            escalation_threshold: escalation_threshold.clamp(0.0, 1.0),
        }
    }
}

#[async_trait]
impl TaskClassifier for LayeredClassifier {
    async fn classify(&self, input: &ClassifierInput<'_>) -> TaskClassification {
        let h = self.heuristic.classify(input).await;
        if h.confidence >= self.escalation_threshold {
            return h;
        }
        if let Some(m) = self.model.as_ref() {
            tracing::debug!(
                heuristic_confidence = h.confidence,
                threshold = self.escalation_threshold,
                "auto-router: escalating to tiny-model classifier",
            );
            return m.classify(input).await;
        }
        h
    }
}
