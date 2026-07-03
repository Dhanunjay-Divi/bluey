//! cue-transcribe — real-time, on-device speech-to-text for Bluey.
//!
//! Cross-platform streaming ASR built on NVIDIA Parakeet/Nemotron (ONNX via
//! parakeet-rs). The engine ([`SttEngine`]) is platform-agnostic; capture is
//! per-OS (added incrementally — see `docs/TRANSCRIBE-BUILD-PLAN.md`).
//!
//! Design rules (learned the hard way):
//! - ONE [`SttEngine`] per audio source (the model is stateful; sharing corrupts).
//! - CPU execution provider by default (stable + fast; CoreML is unstable here).
//! - Speaker label comes from the SOURCE (mic = You, system = They), not diarization.

mod engine;

pub use engine::{SttEngine, TranscriptChunk};

/// Which audio source a transcript came from. Drives the speaker label without
/// any diarization model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The user's microphone — labeled "You".
    Microphone,
    /// System / meeting audio (the other participants) — labeled "They".
    System,
}

impl Source {
    /// The human-facing speaker label for this source.
    pub fn label(self) -> &'static str {
        match self {
            Source::Microphone => "You",
            Source::System => "They",
        }
    }
}
