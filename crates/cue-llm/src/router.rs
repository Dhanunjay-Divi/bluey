use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tracing::warn;

use crate::{LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};

/// Failover router: tries providers in order, advancing on Auth/Quota errors.
pub struct LlmRouter {
    providers: Vec<Box<dyn LlmProvider>>,
    active: AtomicUsize,
}

impl LlmRouter {
    pub fn new(providers: Vec<Box<dyn LlmProvider>>) -> Self {
        Self {
            providers,
            active: AtomicUsize::new(0),
        }
    }

    pub fn provider_names(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }
}

#[async_trait]
impl LlmProvider for LlmRouter {
    fn name(&self) -> &'static str {
        "router"
    }

    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        if self.providers.is_empty() {
            return Err(LlmError::Provider("no providers configured".into()));
        }
        let start = self.active.load(Ordering::Relaxed);
        let len = self.providers.len();
        let mut last_err = None;

        for i in 0..len {
            let idx = (start + i) % len;
            let provider = &self.providers[idx];
            match provider.complete(req).await {
                Ok(resp) => {
                    self.active.store(idx, Ordering::Relaxed);
                    return Ok(resp);
                }
                Err(e) if e.should_failover() => {
                    warn!(provider = provider.name(), error = %e, "failover");
                    last_err = Some(e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.unwrap_or(LlmError::Provider("all providers failed".into())))
    }

    async fn complete_stream(&self, req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        if self.providers.is_empty() {
            return Err(LlmError::Provider("no providers configured".into()));
        }
        let start = self.active.load(Ordering::Relaxed);
        let len = self.providers.len();
        let mut last_err = None;

        for i in 0..len {
            let idx = (start + i) % len;
            let provider = &self.providers[idx];
            match provider.complete_stream(req).await {
                Ok(stream) => {
                    self.active.store(idx, Ordering::Relaxed);
                    return Ok(stream);
                }
                Err(e) if e.should_failover() => {
                    warn!(provider = provider.name(), error = %e, "stream failover");
                    last_err = Some(e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.unwrap_or(LlmError::Provider("all providers failed".into())))
    }

    fn supports_streaming(&self) -> bool {
        self.providers.iter().any(|p| p.supports_streaming())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmError, LlmRequest, LlmResponse};

    struct MockProvider {
        name: &'static str,
        result: Result<LlmResponse, LlmError>,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            match &self.result {
                Ok(r) => Ok(r.clone()),
                Err(LlmError::Auth) => Err(LlmError::Auth),
                Err(LlmError::Quota(s)) => Err(LlmError::Quota(s.clone())),
                Err(LlmError::Network(s)) => Err(LlmError::Network(s.clone())),
                Err(LlmError::Provider(s)) => Err(LlmError::Provider(s.clone())),
                Err(LlmError::CapacityBusy {
                    retry_after_secs,
                    reason,
                }) => Err(LlmError::CapacityBusy {
                    retry_after_secs: *retry_after_secs,
                    reason: reason.clone(),
                }),
                Err(LlmError::Billing(s)) => Err(LlmError::Billing(s.clone())),
            }
        }
    }

    fn llm_response(text: impl Into<String>) -> LlmResponse {
        LlmResponse {
            text: text.into(),
            cost: None,
            cost_label: None,
            artifact: None,
            sources: Vec::new(),
        }
    }

    fn test_req() -> LlmRequest {
        LlmRequest {
            system: String::new(),
            user: "hi".into(),
            session_id: None,
            max_tokens: None,
            temperature: None,
            reasoning_effort: None,
            thinking_budget_tokens: None,
            request_id: None,
            image_data_urls: Vec::new(),
            context: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_first_provider_succeeds() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "a",
                result: Ok(llm_response("from a")),
            }),
            Box::new(MockProvider {
                name: "b",
                result: Ok(llm_response("from b")),
            }),
        ]);
        let resp = router.complete(&test_req()).await.unwrap();
        assert_eq!(resp.text, "from a");
    }

    #[tokio::test]
    async fn test_failover_on_auth() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "a",
                result: Err(LlmError::Auth),
            }),
            Box::new(MockProvider {
                name: "b",
                result: Ok(llm_response("from b")),
            }),
        ]);
        let resp = router.complete(&test_req()).await.unwrap();
        assert_eq!(resp.text, "from b");
    }

    #[tokio::test]
    async fn test_failover_on_quota() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "a",
                result: Err(LlmError::Quota("limit".into())),
            }),
            Box::new(MockProvider {
                name: "b",
                result: Ok(llm_response("from b")),
            }),
        ]);
        let resp = router.complete(&test_req()).await.unwrap();
        assert_eq!(resp.text, "from b");
    }

    #[tokio::test]
    async fn test_no_failover_on_network() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "a",
                result: Err(LlmError::Network("timeout".into())),
            }),
            Box::new(MockProvider {
                name: "b",
                result: Ok(llm_response("from b")),
            }),
        ]);
        let err = router.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Network(_)));
    }

    #[tokio::test]
    async fn test_no_failover_on_capacity_busy() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "managed",
                result: Err(LlmError::CapacityBusy {
                    retry_after_secs: 15,
                    reason: "provider_key_cooling_down".into(),
                }),
            }),
            Box::new(MockProvider {
                name: "direct",
                result: Ok(llm_response("should never be reached")),
            }),
        ]);
        let err = router.complete(&test_req()).await.unwrap_err();
        match err {
            LlmError::CapacityBusy {
                retry_after_secs,
                reason,
            } => {
                assert_eq!(retry_after_secs, 15);
                assert_eq!(reason, "provider_key_cooling_down");
            }
            other => panic!("expected CapacityBusy, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_all_fail() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "a",
                result: Err(LlmError::Auth),
            }),
            Box::new(MockProvider {
                name: "b",
                result: Err(LlmError::Quota("done".into())),
            }),
        ]);
        let err = router.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Quota(_)));
    }

    #[tokio::test]
    async fn test_empty_router() {
        let router = LlmRouter::new(vec![]);
        let err = router.complete(&test_req()).await.unwrap_err();
        assert!(matches!(err, LlmError::Provider(_)));
    }

    #[test]
    fn test_provider_names() {
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "x",
                result: Ok(llm_response("")),
            }),
            Box::new(MockProvider {
                name: "y",
                result: Ok(llm_response("")),
            }),
        ]);
        assert_eq!(router.provider_names(), vec!["x", "y"]);
    }

    #[tokio::test]
    async fn test_stream_default_fallback() {
        use futures_util::StreamExt;
        let router = LlmRouter::new(vec![Box::new(MockProvider {
            name: "a",
            result: Ok(llm_response("hello")),
        })]);
        let mut stream = router.complete_stream(&test_req()).await.unwrap();
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.text, "hello");
        assert!(chunk.finished);
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn billing_error_does_not_failover() {
        // Codex Stage 5 S5.2: a Billing error from a managed provider
        // must NOT failover to a direct provider. The router returns
        // the Billing error to the caller terminal.
        let router = LlmRouter::new(vec![
            Box::new(MockProvider {
                name: "managed",
                result: Err(LlmError::Billing("balance $0.00 insufficient".into())),
            }),
            Box::new(MockProvider {
                name: "openai-direct",
                result: Ok(llm_response("should never be reached")),
            }),
        ]);
        let err = router.complete(&test_req()).await.unwrap_err();
        match err {
            LlmError::Billing(msg) => assert!(msg.contains("insufficient")),
            other => panic!("expected Billing, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn billing_error_terminal_helpers() {
        let e = LlmError::Billing("test".into());
        assert!(!e.should_failover());
        assert!(!e.is_retryable());
    }

    #[tokio::test]
    async fn capacity_busy_terminal_helpers() {
        let e = LlmError::CapacityBusy {
            retry_after_secs: 30,
            reason: "upstream_spend_guard".into(),
        };
        assert!(!e.should_failover());
        assert!(!e.is_retryable());
    }
}
