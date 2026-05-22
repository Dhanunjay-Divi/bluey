//! Integration test for auto-recap on session end using a MockLlm provider.

use async_trait::async_trait;
use cue_llm::{LlmError, LlmProvider, LlmRequest, LlmResponse};

use cue_daemon::llm::RecapLlm;

/// A mock LLM provider that returns a fixed recap response.
struct MockLlm;

#[async_trait]
impl LlmProvider for MockLlm {
    fn name(&self) -> &'static str {
        "mock"
    }
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        assert!(
            req.system.contains("Summarize"),
            "RecapLlm should use summarize system prompt"
        );
        Ok(LlmResponse {
            text: format!(
                "## Recap\n- Discussed: {}",
                &req.user[..req.user.len().min(40)]
            ),
            cost: None,
            cost_label: None,
            artifact: None,
        })
    }
}

#[tokio::test]
async fn recap_llm_produces_cue_response_with_mock() {
    let transcript = "Alice: We need to ship by Friday.\nBob: Agreed, let us finalize the API.";
    let session_id = "test-session-001";

    let resp = RecapLlm
        .run(transcript, session_id, &MockLlm)
        .await
        .unwrap();

    assert_eq!(resp.kind, "recap");
    assert_eq!(resp.source_session_id, session_id);
    assert!(resp.text.contains("Recap"));
    assert!(resp.text.contains("Discussed"));
    assert!(resp.source_text.is_none());
    assert!(resp.ts_ms > 0);
}

#[tokio::test]
async fn recap_llm_error_propagates() {
    struct FailLlm;

    #[async_trait]
    impl LlmProvider for FailLlm {
        fn name(&self) -> &'static str {
            "fail"
        }
        async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            Err(LlmError::Network("timeout".into()))
        }
    }

    let result = RecapLlm.run("some transcript", "sess-x", &FailLlm).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn recap_response_has_unique_id() {
    let r1 = RecapLlm.run("a", "s1", &MockLlm).await.unwrap();
    let r2 = RecapLlm.run("b", "s1", &MockLlm).await.unwrap();
    assert_ne!(r1.id, r2.id);
}
