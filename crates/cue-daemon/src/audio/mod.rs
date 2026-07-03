//! Audio capture + VAD pipeline for cue-daemon.

pub mod framer;
pub mod vad;

// Meeting audio retention for speaker diarization — only compiled with the
// `diarize` feature, so the default build keeps no audio buffer.
#[cfg(feature = "diarize")]
pub mod retention;

#[cfg(not(target_arch = "wasm32"))]
pub mod capture;

#[cfg(not(target_arch = "wasm32"))]
pub mod system_capture;
