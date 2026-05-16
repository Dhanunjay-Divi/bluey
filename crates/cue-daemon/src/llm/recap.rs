use cue_llm::{LlmProvider, LlmRequest};

use super::CueResponse;

const SYSTEM_PROMPT: &str = "Summarize this meeting transcript. Extract key decisions, action items (with owners if named), and unresolved questions. Format as structured markdown.";

pub struct RecapLlm;

impl RecapLlm {
    pub async fn run(
        &self,
        transcript: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
    ) -> Result<CueResponse, cue_llm::LlmError> {
        let req = LlmRequest {
            system: SYSTEM_PROMPT.to_string(),
            user: transcript.to_string(),
            max_tokens: Some(1024),
            temperature: Some(0.2),
        };
        let resp = llm.complete(&req).await?;
        Ok(CueResponse::new("recap", resp.text, session_id, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use cue_llm::{LlmError, LlmResponse};

    struct FakeLlm;

    #[async_trait]
    impl LlmProvider for FakeLlm {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            assert!(req.system.contains("Summarize"));
            Ok(LlmResponse {
                text: "## Recap\n- Decision: ship it".into(),
            })
        }
    }

    #[tokio::test]
    async fn test_recap_llm() {
        let llm = RecapLlm;
        let resp = llm
            .run("Alice: let's ship it. Bob: agreed.", "sess-2", &FakeLlm)
            .await
            .unwrap();
        assert_eq!(resp.kind, "recap");
        assert!(resp.text.contains("Recap"));
        assert_eq!(resp.source_session_id, "sess-2");
    }
}
