//! Integration tests for end-to-end cue streaming.
//!
//! Verifies that the specialized LLMs emit per-chunk callbacks with DELTA
//! partial_text, and that the final response is consistent.

use async_trait::async_trait;
use cue_llm::{LlmChunk, LlmChunkStream, LlmError, LlmProvider, LlmRequest, LlmResponse};
use std::sync::{Arc, Mutex};

/// A mock LLM provider that emits 3 chunks.
struct MockStreamingLlm;

#[async_trait]
impl LlmProvider for MockStreamingLlm {
    fn name(&self) -> &'static str {
        "mock-streaming"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse {
            text: "Hello world!".into(),
            cost: None,
            cost_label: None,
            artifact: None,
        })
    }

    async fn complete_stream(&self, _req: &LlmRequest) -> Result<LlmChunkStream, LlmError> {
        let chunks = vec![
            Ok(LlmChunk {
                text: "Hello".into(),
                finished: false,
                cost: None,
                cost_label: None,
                artifact: None,
            }),
            Ok(LlmChunk {
                text: " world".into(),
                finished: false,
                cost: None,
                cost_label: None,
                artifact: None,
            }),
            Ok(LlmChunk {
                text: "!".into(),
                finished: true,
                cost: None,
                cost_label: None,
                artifact: None,
            }),
        ];
        Ok(Box::pin(futures_util::stream::iter(chunks)))
    }
}

#[derive(Debug, Clone)]
struct ChunkRecord {
    partial_text: String,
    finished: bool,
}

#[tokio::test]
async fn answer_llm_emits_chunks_as_deltas() {
    use cue_daemon::llm::AnswerLlm;

    let chunks: Arc<Mutex<Vec<ChunkRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_clone = chunks.clone();

    let resp = AnswerLlm
        .run_streaming(
            "What is 2+2?",
            "test-session",
            &MockStreamingLlm,
            |partial, finished| {
                chunks_clone.lock().unwrap().push(ChunkRecord {
                    partial_text: partial.to_string(),
                    finished,
                });
            },
        )
        .await
        .unwrap();

    let recorded = chunks.lock().unwrap().clone();
    assert_eq!(recorded.len(), 3, "expected 3 chunk callbacks");

    // Verify delta semantics: each chunk text is the NEW text, not cumulative
    // Daemon emits DELTAS (just the new chunk text), not cumulative.
    // The dashboard appends these to build the full response.
    assert_eq!(recorded[0].partial_text, "Hello");
    assert_eq!(recorded[1].partial_text, " world");
    assert_eq!(recorded[2].partial_text, "!");

    // Verify finished flags
    assert!(!recorded[0].finished);
    assert!(!recorded[1].finished);
    assert!(recorded[2].finished);

    // Final response matches accumulated text
    assert_eq!(resp.text, "Hello world!");
    assert_eq!(resp.kind, "answer");
}

#[tokio::test]
async fn recap_llm_emits_chunks_as_deltas() {
    use cue_daemon::llm::RecapLlm;

    let chunks: Arc<Mutex<Vec<ChunkRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_clone = chunks.clone();

    let resp = RecapLlm
        .run_streaming(
            "Alice said hello. Bob agreed.",
            "test-session",
            &MockStreamingLlm,
            |partial, finished| {
                chunks_clone.lock().unwrap().push(ChunkRecord {
                    partial_text: partial.to_string(),
                    finished,
                });
            },
        )
        .await
        .unwrap();

    let recorded = chunks.lock().unwrap().clone();
    assert_eq!(recorded.len(), 3);
    // Deltas, not cumulative
    assert_eq!(recorded[0].partial_text, "Hello");
    assert_eq!(recorded[1].partial_text, " world");
    assert_eq!(recorded[2].partial_text, "!");
    assert!(recorded[2].finished);
    // Accumulated final response is the concatenation
    let accumulated: String = recorded.iter().map(|r| r.partial_text.as_str()).collect();
    assert_eq!(accumulated, "Hello world!");
    assert_eq!(resp.text, "Hello world!");
    assert_eq!(resp.kind, "recap");
}

#[tokio::test]
async fn suggest_llm_emits_chunks_as_deltas() {
    use cue_daemon::llm::WhatToAnswerLlm;

    let chunks: Arc<Mutex<Vec<ChunkRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_clone = chunks.clone();

    let resp = WhatToAnswerLlm
        .run_streaming(
            "Bob: we need to decide.",
            "test-session",
            &MockStreamingLlm,
            |partial, finished| {
                chunks_clone.lock().unwrap().push(ChunkRecord {
                    partial_text: partial.to_string(),
                    finished,
                });
            },
        )
        .await
        .unwrap();

    let recorded = chunks.lock().unwrap().clone();
    assert_eq!(recorded.len(), 3);
    // Deltas, not cumulative
    assert_eq!(recorded[0].partial_text, "Hello");
    assert_eq!(recorded[1].partial_text, " world");
    assert_eq!(recorded[2].partial_text, "!");
    assert!(recorded[2].finished);
    let accumulated: String = recorded.iter().map(|r| r.partial_text.as_str()).collect();
    assert_eq!(accumulated, "Hello world!");
    assert_eq!(resp.text, "Hello world!");
    assert_eq!(resp.kind, "suggestion");
}

#[tokio::test]
async fn response_id_is_consistent_across_cue_response() {
    use cue_daemon::llm::AnswerLlm;

    // The CueResponse.id is generated inside run_streaming, but in the dashboard
    // command we override it with a pre-generated UUID. Here we verify the
    // CueResponse has a valid UUID id.
    let resp = AnswerLlm
        .run_streaming("test?", "sess-1", &MockStreamingLlm, |_, _| {})
        .await
        .unwrap();

    // id should be a valid UUID
    assert!(uuid::Uuid::parse_str(&resp.id).is_ok());
    assert_eq!(resp.source_session_id, "sess-1");
}

#[tokio::test]
async fn non_streaming_provider_still_calls_callback() {
    use cue_daemon::llm::AnswerLlm;

    struct NonStreamingLlm;

    #[async_trait]
    impl LlmProvider for NonStreamingLlm {
        fn name(&self) -> &'static str {
            "non-streaming"
        }
        async fn complete(&self, _req: &LlmRequest) -> Result<LlmResponse, LlmError> {
            Ok(LlmResponse {
                text: "Final answer.".into(),
                cost: None,
                cost_label: None,
                artifact: None,
            })
        }
    }

    let chunks: Arc<Mutex<Vec<ChunkRecord>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_clone = chunks.clone();

    let resp = AnswerLlm
        .run_streaming("test?", "sess-1", &NonStreamingLlm, |partial, finished| {
            chunks_clone.lock().unwrap().push(ChunkRecord {
                partial_text: partial.to_string(),
                finished,
            });
        })
        .await
        .unwrap();

    let recorded = chunks.lock().unwrap().clone();
    // Non-streaming: single callback with full text and finished=true
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].partial_text, "Final answer.");
    assert!(recorded[0].finished);
    assert_eq!(resp.text, "Final answer.");
}
