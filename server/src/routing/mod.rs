//! Routing module aggregator.

pub mod dispatcher;

pub use dispatcher::{
    complete, embed, resolve_route, transcribe, Completion, EmbedCompletion, TranscribeCompletion,
};
