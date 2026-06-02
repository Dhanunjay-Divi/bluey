//! Routing module aggregator.

pub mod dispatcher;

pub use dispatcher::{
    complete, complete_with_key, effective_max_output_tokens, embed, embed_with_key, resolve_route,
    resolve_route_candidates, resolve_thinking_budget, resolve_transcribe_candidates, transcribe,
    transcribe_with_key, upstream_retry_after, Completion, EmbedCompletion, ThinkingBudget,
    ThinkingMode, TranscribeCompletion,
};
