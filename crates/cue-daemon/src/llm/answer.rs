use cue_llm::{LlmArtifactMetadata, LlmCostMetadata, LlmProvider, LlmRequest};
use futures_util::StreamExt;

use super::CueResponse;

pub const SYSTEM_PROMPT: &str = "\
You are Bluey, a live meeting and work copilot. Return a human-speak talk track \
the user can adapt, not an assistant essay.

Human-speak contract:
- Write in first person when giving an answer the user may say aloud: \"I would...\", \"My approach is...\".
- Prefer a natural spoken flow: acknowledge the question, give the core answer, then add the reason or example.
- Do not invent personal experience, shipped work, metrics, or ownership that is not in the question or session context.
- If the user asks about a plan or approach, phrase it as what they would do, not what you as an AI would do.
- No assistant preamble such as \"Sure\", \"Here is\", \"As an AI\", or \"You can say\".
- Do not sound like a polished memo: avoid source labels, repeated headings, and long markdown checklists in the chat answer.
- Keep it speakable: 2-5 concise sentences by default, with short bullets only when useful.";

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
            reasoning_effort: None,
            thinking_budget_tokens: None,
            request_id: None,
        };
        let mut cost: Option<LlmCostMetadata> = None;
        let mut cost_label: Option<String> = None;
        let mut artifact: Option<LlmArtifactMetadata> = None;
        let text = if llm.supports_streaming() {
            let mut stream = llm.complete_stream(&req).await?;
            let mut acc = String::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                if chunk.cost.is_some() {
                    cost = chunk.cost.clone();
                }
                if chunk.cost_label.is_some() {
                    cost_label = chunk.cost_label.clone();
                }
                if chunk.artifact.is_some() {
                    artifact = chunk.artifact.clone();
                }
                // Emit DELTA (just the new text), not cumulative — the dashboard appends.
                on_chunk(&chunk.text, chunk.finished);
                acc.push_str(&chunk.text);
                if chunk.finished {
                    break;
                }
            }
            acc
        } else {
            let resp = llm.complete(&req).await?;
            cost = resp.cost.clone();
            cost_label = resp.cost_label.clone();
            artifact = resp.artifact.clone();
            on_chunk(&resp.text, true);
            resp.text
        };
        Ok(
            CueResponse::new("answer", text, session_id, Some(question.to_string()))
                .with_llm_metadata(cost.as_ref(), cost_label.as_deref(), artifact.as_ref()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use cue_llm::{LlmArtifactMetadata, LlmCostMetadata, LlmError, LlmResponse};

    struct FakeLlm;

    #[async_trait]
    impl LlmProvider for FakeLlm {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            assert!(req.system.contains("meeting"));
            assert!(req.system.contains("Human-speak contract"));
            assert!(req.system.contains("first person"));
            assert!(req.system.contains("Do not invent personal experience"));
            assert!(req.system.contains("Do not sound like a polished memo"));
            Ok(LlmResponse {
                text: "The answer is 42.".into(),
                cost: None,
                cost_label: None,
                artifact: None,
            })
        }
    }

    struct CostedLlm;

    #[async_trait]
    impl LlmProvider for CostedLlm {
        fn name(&self) -> &'static str {
            "costed"
        }
        async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            Ok(LlmResponse {
                text: "Costed answer.".into(),
                cost: Some(LlmCostMetadata {
                    provider: "openai".into(),
                    model: "gpt-4o-mini".into(),
                    input_tokens: 10,
                    output_tokens: 5,
                    cost_cents: 2,
                    balance_cents_after: Some(2998),
                    trial_seconds_remaining: Some(0),
                }),
                cost_label: Some("$0.02 · balance $29.98".into()),
                artifact: Some(LlmArtifactMetadata {
                    artifact_type: "code".into(),
                    body: "CODE\n----\nfn answer() -> i32 { 42 }".into(),
                    confidence: Some(0.96),
                }),
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

    #[tokio::test]
    async fn test_answer_preserves_cost_metadata() {
        let llm = AnswerLlm;
        let resp = llm.run("Question?", "sess-cost", &CostedLlm).await.unwrap();
        assert_eq!(resp.text, "Costed answer.");
        assert_eq!(resp.cost_cents, Some(2));
        assert_eq!(resp.balance_cents_after, Some(2998));
        assert_eq!(resp.provider.as_deref(), Some("openai"));
        assert_eq!(resp.model.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(resp.cost_label.as_deref(), Some("$0.02 · balance $29.98"));
        assert_eq!(resp.artifact_type.as_deref(), Some("code"));
        assert!(resp
            .artifact_body
            .as_deref()
            .is_some_and(|body| body.contains("fn answer")));
        assert_eq!(resp.artifact_confidence, Some(0.96));
    }
}
