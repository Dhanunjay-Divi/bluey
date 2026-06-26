//! Speculative routing: stream the Instant draft immediately, refine with a
//! Deep answer when it completes.
//!
//! Product behaviour:
//!
//! 1. Classifier says "Hard" / Deep lane.
//! 2. Speculative router fires the Deep request in the background.
//! 3. Speculative router ALSO fires the Instant lane in parallel and yields
//!    its chunks to the caller as `SpeculativeChunk::Draft { text }` until the
//!    Instant stream finishes.
//! 4. When the Deep request completes, the router emits a single
//!    `SpeculativeChunk::Final { text }` carrying the deep answer's full text.
//!    The dashboard / overlay treats this as a replace (the existing
//!    `OverlayCommand::UpdateCard` already supports replacing a card body in
//!    full).
//!
//! The Instant lane stays cheap so the wasted spend is bounded: instant
//! provider + model are configured to the cheapest streaming option.
//!
//! `SpeculativeRouter` is provider-agnostic. The daemon/dashboard command layer
//! wires it to live providers through `cue_llm::LlmRouter` and
//! `ProviderRegistry`; tests in this crate exercise the streaming contract via
//! mock providers.

use std::sync::Arc;

use async_trait::async_trait;
use cue_llm::{LlmArtifactMetadata, LlmChunk, LlmCostMetadata, LlmError, LlmProvider, LlmRequest};
use futures_util::{Stream, StreamExt};

use crate::model::ProviderRoute;
use crate::policy::RoutingPolicy;
use crate::TaskClassification;

/// One unit of output from the speculative router.
#[derive(Debug, Clone, PartialEq)]
pub enum SpeculativeChunk {
    /// Streaming draft from the Instant lane. Concatenate as deltas.
    Draft {
        /// Delta text for this chunk.
        text: String,
        /// True on the final draft chunk.
        finished: bool,
        /// Optional managed billing metadata emitted on the terminal chunk.
        cost: Option<LlmCostMetadata>,
        /// Optional customer-facing cost label emitted by managed billing.
        cost_label: Option<String>,
        /// Optional server-classified canvas artifact.
        artifact: Option<LlmArtifactMetadata>,
    },
    /// Replacement final answer from the Deep lane. Replace the entire card body.
    Final {
        /// Full final answer text. Replace any draft body with this.
        text: String,
        /// Optional managed billing metadata for the final lane.
        cost: Option<LlmCostMetadata>,
        /// Optional customer-facing cost label emitted by managed billing.
        cost_label: Option<String>,
        /// Optional server-classified canvas artifact.
        artifact: Option<LlmArtifactMetadata>,
    },
    /// A non-recoverable error from one or both lanes.
    Error {
        /// Lane that failed ("draft" or "deep").
        lane: &'static str,
        /// Underlying error message.
        message: String,
    },
}

/// Trait for the LLM dispatcher the speculative router calls into. Wired in
/// the daemon to `cue_llm::LlmRouter`.
#[async_trait]
pub trait SpeculativeProvider: Send + Sync {
    /// Resolve a `ProviderRoute` to a concrete provider implementation.
    async fn provider_for(&self, route: &ProviderRoute) -> Result<Arc<dyn LlmProvider>, LlmError>;
}

/// Speculative router. Configurable: callers decide whether the router path and
/// parallel draft lane are active. The product default uses Auto routing and one
/// selected lane; draft+deep replacement is retained behind
/// `BLUEY_PARALLEL_DRAFTS=1` for latency experiments so normal users see one
/// stable answer card.
pub struct SpeculativeRouter {
    policy: Arc<dyn RoutingPolicy>,
    provider: Arc<dyn SpeculativeProvider>,
    /// When true, Hard / Deep classifications also fire an Instant lane in
    /// parallel and the caller gets a draft+final stream.
    pub speculative_when_deep: bool,
}

impl SpeculativeRouter {
    /// Construct.
    pub fn new(
        policy: Arc<dyn RoutingPolicy>,
        provider: Arc<dyn SpeculativeProvider>,
        speculative_when_deep: bool,
    ) -> Self {
        Self {
            policy,
            provider,
            speculative_when_deep,
        }
    }

    /// Run a classified request and return a stream of speculative chunks.
    ///
    /// If the policy returns the Deep lane and `speculative_when_deep` is
    /// true, this fires both the Deep and Instant lanes; the Instant lane's
    /// chunks are yielded immediately as `Draft`, and once the Deep lane
    /// completes a single `Final` is yielded with the full deep text.
    ///
    /// In all other cases (Instant or Balanced lanes; or Deep without
    /// speculation), only the chosen lane is fired and its chunks are yielded
    /// as `Draft` with the final chunk carrying `finished: true`.
    pub async fn run(
        &self,
        classification: &TaskClassification,
        request: LlmRequest,
    ) -> Result<impl Stream<Item = SpeculativeChunk> + Send, LlmError> {
        let primary_route = self.policy.route(classification);
        let primary_provider = self.provider.provider_for(&primary_route).await?;

        // Decide whether to speculate.
        let should_speculate = self.speculative_when_deep
            && matches!(primary_route.lane, crate::model::ProviderLane::Deep);

        // Channel to fan in chunks from both lanes.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        if should_speculate {
            // Try to build the Instant draft lane. If the Instant provider is
            // unavailable (auth failure, quota, missing key), we MUST still
            // run the Deep lane on its own — dropping the Hard answer just
            // because the cheap draft is unreachable would be a regression.
            let instant_class = TaskClassification {
                latency_lane: crate::model::LatencyLane::Instant,
                ..classification.clone()
            };
            let instant_route = self.policy.route(&instant_class);
            let request = Arc::new(request);
            match self.provider.provider_for(&instant_route).await {
                Ok(instant_provider) => {
                    spawn_lane(
                        LaneRole::Draft,
                        instant_provider,
                        request.clone(),
                        instant_route,
                        tx.clone(),
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "speculative router: instant lane unavailable; running deep-only",
                    );
                    // Surface a non-fatal Error chunk so the UI can show a
                    // subtle "draft skipped" indicator if it wants to.
                    let _ = tx.send(SpeculativeChunk::Error {
                        lane: "draft",
                        message: format!("instant lane unavailable: {e}"),
                    });
                }
            }
            spawn_lane(LaneRole::Deep, primary_provider, request, primary_route, tx);
        } else {
            // Single-lane: emit chunks as drafts (callers treat all chunks as
            // deltas + the final-flagged chunk as completion).
            let request = Arc::new(request);
            spawn_lane(
                LaneRole::Draft,
                primary_provider,
                request,
                primary_route,
                tx,
            );
        }

        // Adapt the unbounded receiver into a Stream.
        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|chunk| (chunk, rx))
        });
        Ok(stream)
    }
}

#[derive(Clone, Copy)]
enum LaneRole {
    /// Draft lane (Instant): chunks emit as `SpeculativeChunk::Draft`.
    Draft,
    /// Deep lane: completion emits as a single `SpeculativeChunk::Final`.
    Deep,
}

impl LaneRole {
    fn label(self) -> &'static str {
        match self {
            LaneRole::Draft => "draft",
            LaneRole::Deep => "deep",
        }
    }
}

fn spawn_lane(
    role: LaneRole,
    provider: Arc<dyn LlmProvider>,
    request: Arc<LlmRequest>,
    route: ProviderRoute,
    tx: tokio::sync::mpsc::UnboundedSender<SpeculativeChunk>,
) {
    tokio::spawn(async move {
        // Codex Stage 9 round-2 Blocker 3: lane-scoped idempotency keys.
        // Draft + Final are TWO distinct provider calls; if they share
        // the same request_id, the server idempotency row collides
        // (second call returns InProgress or CachedComplete instead of
        // running). Append ":draft" or ":final" so each lane has its
        // own server-side idempotency cell. Caller-side correlation is
        // preserved by the shared logical prefix.
        let lane_request_id = request.request_id.as_ref().map(|id| {
            let suffix = match role {
                LaneRole::Draft => ":draft",
                LaneRole::Deep => ":final",
            };
            format!("{id}{suffix}")
        });
        let req = LlmRequest {
            system: request.system.clone(),
            user: request.user.clone(),
            session_id: request.session_id.clone(),
            max_tokens: route.max_tokens.or(request.max_tokens),
            temperature: route.temperature.or(request.temperature),
            reasoning_effort: request.reasoning_effort.clone(),
            thinking_budget_tokens: request.thinking_budget_tokens,
            request_id: lane_request_id,
            image_data_urls: request.image_data_urls.clone(),
        };
        // Honor route.stream: if the lane wants streaming, use complete_stream;
        // otherwise use complete() and emit a single synthetic chunk in the
        // shape appropriate for the role.
        if route.stream {
            match provider.complete_stream(&req).await {
                Ok(mut stream) => {
                    let mut accumulated = String::new();
                    let mut cost = None;
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(LlmChunk {
                                text,
                                finished,
                                cost: chunk_cost,
                                cost_label,
                                artifact,
                                status: _,
                                sources: _,
                            }) => match role {
                                LaneRole::Draft => {
                                    let _ = tx.send(SpeculativeChunk::Draft {
                                        text,
                                        finished,
                                        cost: chunk_cost,
                                        cost_label,
                                        artifact,
                                    });
                                    if finished {
                                        break;
                                    }
                                }
                                LaneRole::Deep => {
                                    accumulated.push_str(&text);
                                    if chunk_cost.is_some() {
                                        cost = chunk_cost;
                                    }
                                    if finished {
                                        let _ = tx.send(SpeculativeChunk::Final {
                                            text: accumulated,
                                            cost,
                                            cost_label,
                                            artifact,
                                        });
                                        return;
                                    }
                                }
                            },
                            Err(e) => {
                                let _ = tx.send(SpeculativeChunk::Error {
                                    lane: role.label(),
                                    message: e.to_string(),
                                });
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(SpeculativeChunk::Error {
                        lane: role.label(),
                        message: e.to_string(),
                    });
                }
            }
        } else {
            match provider.complete(&req).await {
                Ok(resp) => match role {
                    LaneRole::Draft => {
                        let _ = tx.send(SpeculativeChunk::Draft {
                            text: resp.text,
                            finished: true,
                            cost: resp.cost,
                            cost_label: resp.cost_label,
                            artifact: resp.artifact,
                        });
                    }
                    LaneRole::Deep => {
                        let _ = tx.send(SpeculativeChunk::Final {
                            text: resp.text,
                            cost: resp.cost,
                            cost_label: resp.cost_label,
                            artifact: resp.artifact,
                        });
                    }
                },
                Err(e) => {
                    let _ = tx.send(SpeculativeChunk::Error {
                        lane: role.label(),
                        message: e.to_string(),
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContextNeeds, Difficulty, LatencyLane, ProviderLane, TaskType};
    use crate::policy::StaticPolicy;
    use cue_llm::{LlmChunkStream, LlmRequest, LlmResponse};
    use futures_util::stream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock provider that yields a fixed sequence of chunks for streaming
    /// and a fixed text for non-streaming.
    struct MockProvider {
        chunks: Vec<&'static str>,
        deep_text: &'static str,
        completion_calls: AtomicUsize,
        stream_calls: AtomicUsize,
    }
    impl MockProvider {
        fn new(chunks: Vec<&'static str>, deep_text: &'static str) -> Self {
            Self {
                chunks,
                deep_text,
                completion_calls: AtomicUsize::new(0),
                stream_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn supports_streaming(&self) -> bool {
            true
        }
        async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            self.completion_calls.fetch_add(1, Ordering::Relaxed);
            Ok(LlmResponse {
                text: self.deep_text.to_string(),
                cost: None,
                cost_label: None,
                artifact: None,
                sources: Vec::new(),
            })
        }
        async fn complete_stream(&self, _req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
            self.stream_calls.fetch_add(1, Ordering::Relaxed);
            let chunks: Vec<_> = self
                .chunks
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    Ok(LlmChunk {
                        text: c.to_string(),
                        finished: i == self.chunks.len() - 1,
                        cost: None,
                        cost_label: None,
                        artifact: None,
                        status: None,
                        sources: Vec::new(),
                    })
                })
                .collect();
            Ok(Box::pin(stream::iter(chunks)))
        }
    }

    /// Mock dispatcher returning the same provider for every route.
    struct MockDispatch {
        provider: Arc<MockProvider>,
    }
    #[async_trait::async_trait]
    impl SpeculativeProvider for MockDispatch {
        async fn provider_for(
            &self,
            _route: &ProviderRoute,
        ) -> Result<Arc<dyn LlmProvider>, LlmError> {
            Ok(self.provider.clone())
        }
    }

    fn classification(lane: LatencyLane) -> TaskClassification {
        TaskClassification {
            task_type: TaskType::General,
            difficulty: match lane {
                LatencyLane::Deep => Difficulty::Hard,
                LatencyLane::Balanced => Difficulty::Medium,
                LatencyLane::Instant => Difficulty::Easy,
            },
            needed_context: ContextNeeds::default(),
            latency_lane: lane,
            confidence: 0.9,
        }
    }

    fn req() -> LlmRequest {
        LlmRequest {
            system: "s".into(),
            user: "u".into(),
            session_id: None,
            max_tokens: None,
            temperature: None,
            reasoning_effort: None,
            thinking_budget_tokens: None,
            request_id: None,
            image_data_urls: Vec::new(),
        }
    }

    #[tokio::test]
    async fn single_lane_emits_draft_chunks() {
        let provider = Arc::new(MockProvider::new(vec!["Hello", " world"], "deep"));
        let dispatch = Arc::new(MockDispatch {
            provider: provider.clone(),
        });
        let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
        let router = SpeculativeRouter::new(policy, dispatch, false);

        let class = classification(LatencyLane::Balanced);
        let stream = router.run(&class, req()).await.unwrap();
        let chunks: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(chunks.len(), 2);
        assert!(
            matches!(chunks[0], SpeculativeChunk::Draft { ref text, finished: false, .. } if text == "Hello")
        );
        assert!(
            matches!(chunks[1], SpeculativeChunk::Draft { ref text, finished: true, .. } if text == " world")
        );
        assert_eq!(provider.stream_calls.load(Ordering::Relaxed), 1);
        assert_eq!(provider.completion_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn speculative_deep_emits_draft_then_final() {
        let provider = Arc::new(MockProvider::new(
            vec!["draft-part-1", "draft-part-2"],
            "DEEP_FINAL_ANSWER",
        ));
        let dispatch = Arc::new(MockDispatch {
            provider: provider.clone(),
        });
        let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
        let router = SpeculativeRouter::new(policy, dispatch, true);

        let class = classification(LatencyLane::Deep);
        let stream = router.run(&class, req()).await.unwrap();
        let chunks: Vec<_> = stream.collect::<Vec<_>>().await;

        // We expect 2 drafts + 1 final, in some order; assert their content.
        let mut draft_texts: Vec<String> = vec![];
        let mut final_text: Option<String> = None;
        for c in chunks {
            match c {
                SpeculativeChunk::Draft { text, .. } => draft_texts.push(text),
                SpeculativeChunk::Final { text, .. } => {
                    final_text = Some(text);
                }
                SpeculativeChunk::Error { lane, message } => panic!("error from {lane}: {message}"),
            }
        }
        assert_eq!(draft_texts, vec!["draft-part-1", "draft-part-2"]);
        assert_eq!(final_text.as_deref(), Some("draft-part-1draft-part-2"));
        // Deep routes are streaming now, so speculation starts one stream for
        // the instant draft and one stream for the deep final.
        assert_eq!(provider.stream_calls.load(Ordering::Relaxed), 2);
        assert_eq!(provider.completion_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn deep_without_speculation_does_not_fire_draft() {
        let provider = Arc::new(MockProvider::new(vec!["should-not-stream"], "DEEP"));
        let dispatch = Arc::new(MockDispatch {
            provider: provider.clone(),
        });
        let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
        let router = SpeculativeRouter::new(policy, dispatch, false);

        let class = classification(LatencyLane::Deep);
        let stream = router.run(&class, req()).await.unwrap();
        let chunks: Vec<_> = stream.collect::<Vec<_>>().await;
        // With speculation off, the Deep lane streams as the single visible
        // answer path. There should be NO replacement Final chunk.
        for c in &chunks {
            assert!(
                !matches!(c, SpeculativeChunk::Final { .. }),
                "unexpected Final without speculation"
            );
        }
    }

    /// Mock dispatcher that fails on Instant routes but succeeds on Deep.
    struct InstantFailDispatch {
        deep_provider: Arc<MockProvider>,
    }
    #[async_trait::async_trait]
    impl SpeculativeProvider for InstantFailDispatch {
        async fn provider_for(
            &self,
            route: &ProviderRoute,
        ) -> Result<Arc<dyn LlmProvider>, LlmError> {
            match route.lane {
                ProviderLane::Instant => Err(LlmError::Provider("instant lane unavailable".into())),
                _ => Ok(self.deep_provider.clone()),
            }
        }
    }

    #[tokio::test]
    async fn deep_runs_even_when_instant_lane_unavailable() {
        // Regression: codex flagged that the previous run() impl returned `?`
        // on Instant provider failure, killing the Deep lane too. We must
        // surface the Deep answer even if the cheap draft is gone.
        let deep = Arc::new(MockProvider::new(vec!["DEEP_ANSWER"], "DEEP_ANSWER"));
        let dispatch = Arc::new(InstantFailDispatch {
            deep_provider: deep.clone(),
        });
        let policy: Arc<dyn RoutingPolicy> = Arc::new(StaticPolicy::defaults());
        let router = SpeculativeRouter::new(policy, dispatch, true);

        let class = classification(LatencyLane::Deep);
        let stream = router.run(&class, req()).await.expect("must not fail");
        let chunks: Vec<_> = stream.collect::<Vec<_>>().await;

        // Must have at least one Final, and a non-fatal Error for the missing draft.
        let has_final = chunks
            .iter()
            .any(|c| matches!(c, SpeculativeChunk::Final { .. }));
        let has_draft_error = chunks
            .iter()
            .any(|c| matches!(c, SpeculativeChunk::Error { lane: "draft", .. }));
        assert!(
            has_final,
            "Deep lane must still produce Final: got {chunks:?}"
        );
        assert!(
            has_draft_error,
            "draft-failure must surface as non-fatal Error chunk"
        );
    }

    #[tokio::test]
    async fn honors_stream_false_on_draft_lane_emits_single_chunk() {
        // Build a route with stream:false. The lane spawner must use complete()
        // not complete_stream() and emit a single Draft chunk with finished:true.
        let provider = Arc::new(MockProvider::new(
            vec!["should-not-stream-anything"],
            "NON_STREAM_DRAFT",
        ));
        let dispatch = Arc::new(MockDispatch {
            provider: provider.clone(),
        });

        // Custom policy that returns stream:false for all lanes.
        struct NonStreamPolicy;
        impl RoutingPolicy for NonStreamPolicy {
            fn route(&self, _c: &TaskClassification) -> ProviderRoute {
                ProviderRoute {
                    lane: ProviderLane::Balanced,
                    provider_name: "mock".into(),
                    model: "any".into(),
                    max_tokens: None,
                    temperature: None,
                    stream: false,
                }
            }
        }
        let policy: Arc<dyn RoutingPolicy> = Arc::new(NonStreamPolicy);
        let router = SpeculativeRouter::new(policy, dispatch, false);

        let class = classification(LatencyLane::Balanced);
        let chunks: Vec<_> = router.run(&class, req()).await.unwrap().collect().await;
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            SpeculativeChunk::Draft { text, finished, .. } => {
                assert_eq!(text, "NON_STREAM_DRAFT");
                assert!(*finished);
            }
            other => panic!("expected single Draft, got {other:?}"),
        }
        // complete() called, complete_stream() NOT called.
        assert_eq!(provider.completion_calls.load(Ordering::Relaxed), 1);
        assert_eq!(provider.stream_calls.load(Ordering::Relaxed), 0);
    }

    /// Codex Stage 9 round-2 Blocker 3: prove draft and final lanes
    /// receive DIFFERENT request_ids when speculation fires, so the
    /// server idempotency row does not collide.
    #[tokio::test]
    async fn lane_scoped_request_ids_when_speculating_deep() {
        use std::sync::Mutex as StdMutex;

        // Shared collector for the request_ids each lane sees.
        let seen_ids: Arc<StdMutex<Vec<Option<String>>>> = Arc::new(StdMutex::new(Vec::new()));

        struct CapturingProvider {
            seen: Arc<StdMutex<Vec<Option<String>>>>,
            name: &'static str,
        }
        #[async_trait]
        impl LlmProvider for CapturingProvider {
            fn name(&self) -> &'static str {
                self.name
            }
            async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
                self.seen.lock().unwrap().push(req.request_id.clone());
                Ok(LlmResponse {
                    text: format!("from-{}", self.name),
                    cost: None,
                    cost_label: None,
                    artifact: None,
                    sources: Vec::new(),
                })
            }
            async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
                self.seen.lock().unwrap().push(req.request_id.clone());
                let chunk = Ok(LlmChunk {
                    text: format!("from-{}", self.name),
                    finished: true,
                    cost: None,
                    cost_label: None,
                    artifact: None,
                    status: None,
                    sources: Vec::new(),
                });
                Ok(Box::pin(futures_util::stream::once(async move { chunk })))
            }
        }

        struct CapturingRegistry {
            seen: Arc<StdMutex<Vec<Option<String>>>>,
        }
        #[async_trait]
        impl SpeculativeProvider for CapturingRegistry {
            async fn provider_for(
                &self,
                route: &ProviderRoute,
            ) -> Result<Arc<dyn LlmProvider>, LlmError> {
                let name: &'static str = match route.lane {
                    ProviderLane::Instant => "instant",
                    ProviderLane::Deep => "deep",
                    _ => "other",
                };
                Ok(Arc::new(CapturingProvider {
                    seen: self.seen.clone(),
                    name,
                }))
            }
        }

        let policy: Arc<dyn RoutingPolicy> = Arc::new(crate::policy::StaticPolicy::defaults());
        let registry: Arc<dyn SpeculativeProvider> = Arc::new(CapturingRegistry {
            seen: seen_ids.clone(),
        });
        let router = SpeculativeRouter::new(policy, registry, true);

        // Hard classification triggers Deep + speculative draft.
        let classification = TaskClassification {
            task_type: crate::TaskType::General,
            difficulty: crate::Difficulty::Hard,
            needed_context: crate::ContextNeeds::default(),
            latency_lane: LatencyLane::Deep,
            confidence: 0.9,
        };
        let mut req = req();
        req.request_id = Some("logical-id-1".into());

        let mut stream = std::pin::pin!(router.run(&classification, req).await.unwrap());
        // Drain.
        while let Some(_chunk) = stream.next().await {}

        let ids = seen_ids.lock().unwrap().clone();
        assert_eq!(ids.len(), 2, "draft + final lanes should both fire");
        let draft_id = ids
            .iter()
            .find(|id| {
                id.as_deref()
                    .map(|s| s.ends_with(":draft"))
                    .unwrap_or(false)
            })
            .expect("expected a :draft id");
        let final_id = ids
            .iter()
            .find(|id| {
                id.as_deref()
                    .map(|s| s.ends_with(":final"))
                    .unwrap_or(false)
            })
            .expect("expected a :final id");
        assert_ne!(
            draft_id, final_id,
            "draft and final must NOT share the same request_id (idempotency collision)"
        );
        // Both should retain the logical prefix for correlation.
        assert!(draft_id.as_deref().unwrap().starts_with("logical-id-1"));
        assert!(final_id.as_deref().unwrap().starts_with("logical-id-1"));
    }

    /// Codex review S9 round-3 blocker: every-lane-errors should emit
    /// only Error chunks so the dashboard returns Ok(None) and runs
    /// the legacy fallback instead of persisting an empty card.
    #[tokio::test]
    async fn all_lanes_error_yields_only_error_chunks() {
        struct ErroringInner;
        #[async_trait]
        impl LlmProvider for ErroringInner {
            fn name(&self) -> &'static str {
                "erroring"
            }
            async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
                Err(LlmError::Provider("upstream blocked for test".into()))
            }
            async fn complete_stream(&self, _req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
                Err(LlmError::Provider("upstream blocked for test".into()))
            }
        }
        struct AlwaysErrorProvider;
        #[async_trait]
        impl SpeculativeProvider for AlwaysErrorProvider {
            async fn provider_for(
                &self,
                _route: &ProviderRoute,
            ) -> Result<Arc<dyn LlmProvider>, LlmError> {
                Ok(Arc::new(ErroringInner))
            }
        }
        let policy: Arc<dyn RoutingPolicy> = Arc::new(crate::policy::StaticPolicy::defaults());
        let registry: Arc<dyn SpeculativeProvider> = Arc::new(AlwaysErrorProvider);
        let router = SpeculativeRouter::new(policy, registry, true);

        let classification = TaskClassification {
            task_type: crate::TaskType::General,
            difficulty: crate::Difficulty::Hard,
            needed_context: crate::ContextNeeds::default(),
            latency_lane: LatencyLane::Deep,
            confidence: 0.9,
        };
        let mut stream = std::pin::pin!(router.run(&classification, req()).await.unwrap());
        let mut errors = 0;
        let mut texts = 0;
        while let Some(chunk) = stream.next().await {
            match chunk {
                SpeculativeChunk::Draft { .. } | SpeculativeChunk::Final { .. } => texts += 1,
                SpeculativeChunk::Error { .. } => errors += 1,
            }
        }
        assert!(errors >= 1, "expected at least one Error chunk");
        assert_eq!(texts, 0, "no Draft/Final chunks should be produced");
    }
}
