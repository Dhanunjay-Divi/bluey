pub mod app;
pub mod audio;
pub mod cloud;
pub mod db;
// Speaker-diarization orchestration (speakrs two-tier). Feature-gated so the
// default build never links the diarization stack.
#[cfg(feature = "diarize")]
pub mod diarize;
pub mod export;
// Decisions-ledger orchestration: stateless cheap-lane extraction every N turns.
pub mod ledger;
pub mod llm;
pub mod overlay;
pub mod secrets;
pub mod storage;
pub mod stt;
// Rolling-summary orchestration: throwaway agent one-shot every N segments.
pub mod summary;
pub mod util;

/// Test-only re-exports of internal app helpers needed by integration tests.
/// Production code does not use this module — it is purely a test seam.
#[doc(hidden)]
pub mod app_test_hooks {
    pub use crate::app::{validate_and_decode_overlay_line, OverlayLineReject};
}
