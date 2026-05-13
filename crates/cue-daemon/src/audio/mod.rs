//! Audio capture + VAD pipeline for cue-daemon.

pub mod framer;
pub mod vad;

#[cfg(not(target_arch = "wasm32"))]
pub mod capture;
