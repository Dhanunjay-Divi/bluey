// Cross-agent session-history retrieval (the "borrow their reasoning"
// side-channel): builds + owns an in-memory index over other agents' past
// session prose, exposed via the `search_agent_history` MCP tool. Feature-gated
// (reuses the local bge-small embedder from `memory`).
#[cfg(feature = "local-memory")]
pub mod agent_history;
pub mod app;
pub mod audio;
pub mod cloud;
// App-owned in-meeting conversation memory: turn log + rolling fold + assembly.
pub mod conversation;
pub mod db;
// Speaker-diarization orchestration (speakrs two-tier). Feature-gated so the
// default build never links the diarization stack.
#[cfg(feature = "diarize")]
pub mod diarize;
pub mod export;
// Decisions-ledger orchestration: stateless cheap-lane extraction every N turns.
pub mod ledger;
pub mod llm;
// Cross-meeting facts memory (local bge-small embedder + supersede store).
#[cfg(feature = "local-memory")]
pub mod memory;
pub mod overlay;
// Question-vs-statement ONNX classifier (two-stage detection, stage 2).
#[cfg(feature = "local-memory")]
pub mod qdetect;
pub mod secrets;
pub mod storage;
pub mod stt;
// Calendar trigger: fires the warm meeting-backend drive ahead of meetings.
pub mod calendar;
// Rolling-summary orchestration: throwaway agent one-shot every N segments.
pub mod setup_status;
pub mod summary;
pub mod util;

/// Test-only re-exports of internal app helpers needed by integration tests.
/// Production code does not use this module — it is purely a test seam.
#[doc(hidden)]
pub mod app_test_hooks {
    pub use crate::app::{validate_and_decode_overlay_line, OverlayLineReject};
}
