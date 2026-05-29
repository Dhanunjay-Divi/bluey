use cue_llm::{LlmArtifactMetadata, LlmCostMetadata, LlmProvider, LlmRequest};
use futures_util::StreamExt;

use super::CueResponse;

pub const SYSTEM_PROMPT: &str = "Given the recent conversation, suggest a concise next thing the user could say. Output 1-2 short bullet points.";

pub struct WhatToAnswerLlm;

impl WhatToAnswerLlm {
    /// Run the suggestion LLM with an optional per-chunk callback for streaming.
    pub async fn run(
        &self,
        transcript: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
    ) -> Result<CueResponse, cue_llm::LlmError> {
        self.run_streaming(transcript, session_id, llm, |_, _| {})
            .await
    }

    /// Run with a callback invoked on each chunk: (delta_text, finished).
    ///  is the NEW text in this chunk, not the cumulative response.
    /// The caller is responsible for accumulating if needed.
    pub async fn run_streaming(
        &self,
        transcript: &str,
        session_id: &str,
        llm: &dyn LlmProvider,
        on_chunk: impl Fn(&str, bool),
    ) -> Result<CueResponse, cue_llm::LlmError> {
        let req = LlmRequest {
            system: SYSTEM_PROMPT.to_string(),
            user: transcript.to_string(),
            max_tokens: Some(200),
            temperature: Some(0.5),
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
            CueResponse::new("suggestion", text, session_id, Some(transcript.to_string()))
                .with_llm_metadata(cost.as_ref(), cost_label.as_deref(), artifact.as_ref()),
        )
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
                cost: None,
                cost_label: None,
                artifact: None,
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
