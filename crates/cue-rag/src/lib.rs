//! cue-rag: Local RAG (Retrieval-Augmented Generation) for Cue.
//!
//! Provides semantic chunking, embedding via pluggable providers, and
//! vector search over session transcripts. v0.1 uses in-memory cosine
//! similarity with SQLite persistence (sqlite-vec swap is a follow-up).

pub mod chunker;
pub mod embedder;
pub mod store;

pub use chunker::{Chunk, Chunker};
pub use embedder::{EmbeddingError, EmbeddingProvider};
pub use store::{RagHit, RagScope, VectorStore};
