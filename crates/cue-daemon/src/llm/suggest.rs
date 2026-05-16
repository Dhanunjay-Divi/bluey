use cue_llm::{LlmProvider, LlmRequest};

use super::CueResponse;

const SYSTEM_PROMPT: &str = "Given the recent conversation, suggest a concise next thing the user could say. Output 1-2 short bullet points.";

pub struct WhatToAnswerLlm;

impl WhatToAnswerLlm {
    pub async fn run(
        &self,
        transcript: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
    ) -> Result<CueResponse, cue_llm::LlmError> {
        let req = LlmRequest {
            system: SYSTEM_PROMPT.to_string(),
            user: transcript.to_string(),
            max_tokens: Some(200),
            temperature: Some(0.5),
        };
        let resp = llm.complete(&req).await?;
        Ok(CueResponse::new(
            "suggestion",
            resp.text,
            session_id,
            Some(transcript.to_string()),
        ))
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
            assert!(req.system.contains("suggest"));
            Ok(LlmResponse {
                text: "- Ask about timeline\n- Confirm budget".into(),
            })
        }
    }

    #[tokio::test]
    async fn test_suggest_llm() {
        let llm = WhatToAnswerLlm;
        let resp = llm
            .run(
                "Bob: we need to decide on the timeline.",
                "sess-3",
                &FakeLlm,
            )
            .await
            .unwrap();
        assert_eq!(resp.kind, "suggestion");
        assert!(resp.text.contains("timeline"));
        assert_eq!(resp.source_session_id, "sess-3");
    }

    #[tokio::test]
    async fn test_suggest_preserves_source() {
        let llm = WhatToAnswerLlm;
        let input = "Alice: what do you think?";
        let resp = llm.run(input, "sess-4", &FakeLlm).await.unwrap();
        assert_eq!(resp.source_text.unwrap(), input);
    }
}
