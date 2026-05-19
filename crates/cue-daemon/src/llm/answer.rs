use cue_llm::{LlmProvider, LlmRequest};
use futures_util::StreamExt;

use super::CueResponse;

pub const SYSTEM_PROMPT: &str = "You are a helpful assistant during a meeting. The user just heard the following question. Reply concisely in 1-3 sentences.";

pub struct AnswerLlm;

impl AnswerLlm {
    /// Run the answer LLM with an optional per-chunk callback for streaming.
    pub async fn run(
        &self,
        question: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
    ) -> Result<CueResponse, cue_llm::LlmError> {
        self.run_streaming(question, session_id, llm, |_, _| {})
            .await
    }

    /// Run with a callback invoked on each chunk: (delta_text, finished).
    ///  is the NEW text in this chunk, not the cumulative response.
    /// The caller is responsible for accumulating if needed.
    pub async fn run_streaming(
        &self,
        question: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
        on_chunk: impl Fn(&str, bool),
    ) -> Result<CueResponse, cue_llm::LlmError> {
        let req = LlmRequest {
            system: SYSTEM_PROMPT.to_string(),
            user: question.to_string(),
            max_tokens: Some(256),
            temperature: Some(0.3),
        };
        let text = if llm.supports_streaming() {
            let mut stream = llm.complete_stream(&req).await?;
            let mut acc = String::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                // Emit DELTA (just the new text), not cumulative — the dashboard appends.
                on_chunk(&chunk.text, chunk.finished);
                acc.push_str(&chunk.text);
                if chunk.finished {
                    break;
                }
            }
            acc
        } else {
            let resp = llm.complete(&req).await?.text;
            on_chunk(&resp, true);
            resp
        };
        Ok(CueResponse::new(
            "answer",
            text,
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
