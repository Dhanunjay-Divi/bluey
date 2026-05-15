//! Error types for the local Whisper provider.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WhisperError {
    #[error("helper binary not found: {0}")]
    BinaryNotFound(String),
    #[error("helper process failed to start: {0}")]
    SpawnFailed(String),
    #[error("failed to parse NDJSON from helper: {0}")]
    ParseError(String),
}
