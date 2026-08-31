pub mod app;
pub mod audio;
pub mod cloud;
pub mod db;
pub(crate) mod diagnostics;
pub(crate) mod doc_conversion;
pub mod export;
pub mod llm;
pub mod overlay;
pub(crate) mod overlay_hydration;
pub(crate) mod overlay_state;
pub(crate) mod rag_indexer;
pub mod secrets;
pub mod storage;
pub mod stt;
pub mod util;

/// Test-only re-exports of internal app helpers needed by integration tests.
/// Production code does not use this module — it is purely a test seam.
#[doc(hidden)]
pub mod app_test_hooks {
    pub use crate::app::{validate_and_decode_overlay_line, OverlayLineReject};
}
