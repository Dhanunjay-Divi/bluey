//! Routing module aggregator.

pub mod dispatcher;

pub use dispatcher::{
    complete, embed, resolve_route, resolve_route_candidates, transcribe, Completion,
    EmbedCompletion, TranscribeCompletion,
};
