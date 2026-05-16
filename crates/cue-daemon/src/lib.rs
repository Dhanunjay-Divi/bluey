pub mod app;
pub mod audio;
pub mod db;
pub mod export;
pub mod llm;
pub mod overlay;
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
