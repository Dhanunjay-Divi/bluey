//! Audio capture pipeline for cue-daemon.

pub mod framer;

// Software reference-based acoustic echo cancellation: subtracts the system
// (far-end) stream from the microphone before STT, so the far side's voice
// leaking from open speakers into the mic is removed and never mislabeled as the
// user. Always compiled — always-on with a BLUEY_AEC=0 escape hatch.
#[cfg(not(target_arch = "wasm32"))]
pub mod aec;

// Meeting audio retention for speaker diarization — only compiled with the
// `diarize` feature, so the default build keeps no audio buffer.
#[cfg(feature = "diarize")]
pub mod retention;

#[cfg(not(target_arch = "wasm32"))]
pub mod capture;

#[cfg(not(target_arch = "wasm32"))]
pub mod system_capture;
