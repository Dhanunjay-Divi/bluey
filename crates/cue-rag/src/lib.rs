//! cue-rag: Local RAG (Retrieval-Augmented Generation) for Cue.
//!
//! Provides semantic chunking, embedding via pluggable providers, and
//! vector search over session transcripts. v0.1 uses in-memory cosine
//! similarity with SQLite persistence (sqlite-vec swap is a follow-up).

// Reusable in-memory retrieval index over other agents' past session history
// (behind local-embed, like the rest of the embedder path).
#[cfg(feature = "local-embed")]
pub mod agent_history;
pub mod chunker;
pub mod embedder;
// Cross-meeting facts memory (long-term tier): extracted facts + supersede.
pub mod facts;
// Index-time filters for agent-session history (self-prompt leak guard).
pub mod filter;
// Hybrid retrieval scoring (mem0 v3 search port: BM25 + entity boost fusion).
pub mod hybrid;
#[cfg(feature = "local-embed")]
pub mod local_embed;
pub mod store;

#[cfg(feature = "local-embed")]
pub use agent_history::{AgentHistoryIndex, HistoryHit, IndexedChunk};
pub use chunker::{hash_dedup, Chunk, Chunker};
pub use embedder::{EmbeddingError, EmbeddingProvider};
pub use facts::{AddOutcome, FactHit, FactRow, FactsStore, HistoryRow};
pub use filter::is_self_prompt;
#[cfg(feature = "local-embed")]
pub use local_embed::LocalBgeEmbedder;
pub use store::{RagHit, VectorStore};
