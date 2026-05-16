use cue_llm::{LlmProvider, LlmRequest};

use super::CueResponse;

const SYSTEM_PROMPT: &str = "You are a helpful assistant during a meeting. The user just heard the following question. Reply concisely in 1-3 sentences.";

pub struct AnswerLlm;

impl AnswerLlm {
    pub async fn run(
        &self,
        question: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
    ) -> Result<CueResponse, cue_llm::LlmError> {
        let req = LlmRequest {
            system: SYSTEM_PROMPT.to_string(),
            user: question.to_string(),
            max_tokens: Some(256),
            temperature: Some(0.3),
        };
        let resp = llm.complete(&req).await?;
        Ok(CueResponse::new(
            "answer",
            resp.text,
            session_id,
            Some(question.to_string()),
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
            assert!(req.system.contains("meeting"));
            Ok(LlmResponse {
                text: "The answer is 42.".into(),
            })
        }
    }

    #[tokio::test]
    async fn test_answer_llm() {
        let llm = AnswerLlm;
        let resp = llm
            .run("What is the meaning of life?", "sess-1", &FakeLlm)
            .await
            .unwrap();
        assert_eq!(resp.kind, "answer");
        assert_eq!(resp.text, "The answer is 42.");
        assert_eq!(resp.source_session_id, "sess-1");
        assert!(resp.source_text.unwrap().contains("meaning of life"));
    }
}
