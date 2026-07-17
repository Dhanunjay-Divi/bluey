//! Routing module aggregator.

pub mod dispatcher;

pub use dispatcher::{
    complete, complete_stream_with_key, complete_with_key, effective_max_output_tokens, embed,
    embed_batch_with_key, embed_with_key, resolve_route, resolve_route_candidates,
    resolve_route_candidates_with_seed, resolve_thinking_budget, resolve_transcribe_candidates,
    transcribe, transcribe_with_key, upstream_retry_after, upstream_terminal_reason, Completion,
    CompletionEventStream, CompletionStreamEvent, EmbedBatchCompletion, EmbedCompletion,
    StreamingCompletion, ThinkingBudget, ThinkingMode, TranscribeCompletion,
};
