//! Voice Activity Detection (VAD) types used by the capture pipeline.
//!
//! bluey runs two VAD stages before audio is handed to the STT provider:
//!
//! 1. **RMS adaptive threshold** (cheap, ~microseconds). Filters out obvious
//!    silence so WebRTC VAD sees fewer frames.
//! 2. **WebRTC ML VAD** (`webrtc-vad` crate). The production-proven neural
//!    VAD used by Chrome, WebRTC.org, and many STT pipelines. Rejects
//!    typing / fan noise / non-speech sounds that pass the RMS gate.
//!
//! This module only holds the type definitions; the actual `RmsGate` and
//! `WebRtcVad` wrappers live in `cue-daemon::audio::vad` so they can depend
//! on `webrtc-vad`.

use serde::{Deserialize, Serialize};

/// What the VAD pipeline decided to do with an incoming audio frame.
///
/// The distinction between `SendSilence` and `Drop` matters: STT providers
/// can behave better when they see occasional silence frames (they use it
/// to finalize utterances), but seeing continuous silence wastes bandwidth
/// and can trigger per-provider idle-timeouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameAction {
    /// Frame contains speech (or a short trailing-silence after speech) —
    /// forward to STT.
    Send,
    /// Frame is silence but part of an active utterance window — send a
    /// silence marker so the STT can finalize.
    SendSilence,
    /// Frame is silence outside an utterance window — drop entirely.
    Drop,
}

impl FrameAction {
    pub fn should_forward(self) -> bool {
        !matches!(self, FrameAction::Drop)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FrameAction::Send => "send",
            FrameAction::SendSilence => "send-silence",
            FrameAction::Drop => "drop",
        }
    }
}

/// Aggressiveness of the WebRTC VAD (values match the upstream API).
///
/// - `Quality` (0) admits the most frames — best recall, worst precision.
/// - `VeryAggressive` (3) rejects the most non-speech — best precision, can
///   drop quiet speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VadAggressiveness {
    Quality,
    LowBitrate,
    Aggressive,
    VeryAggressive,
}

impl VadAggressiveness {
    pub fn as_u8(self) -> u8 {
        match self {
            VadAggressiveness::Quality => 0,
            VadAggressiveness::LowBitrate => 1,
            VadAggressiveness::Aggressive => 2,
            VadAggressiveness::VeryAggressive => 3,
        }
    }
}

impl Default for VadAggressiveness {
    fn default() -> Self {
        VadAggressiveness::Aggressive
    }
}

/// Runtime configuration for the two-stage VAD.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VadConfig {
    pub aggressiveness: VadAggressiveness,
    /// Adaptive RMS starts at `rms_threshold_start` and slowly moves toward
    /// the running noise floor. Expressed as a fraction of i16 max (0.0-1.0).
    pub rms_threshold_start: f32,
    /// How many consecutive silence frames before switching from Send to
    /// SendSilence → Drop. At 20 ms frames, 25 = 500 ms of trailing silence.
    pub silence_hangover_frames: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            aggressiveness: VadAggressiveness::Aggressive,
            rms_threshold_start: 0.02,
            silence_hangover_frames: 25,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_action_should_forward() {
        assert!(FrameAction::Send.should_forward());
        assert!(FrameAction::SendSilence.should_forward());
        assert!(!FrameAction::Drop.should_forward());
    }

    #[test]
    fn vad_aggressiveness_u8_values_match_webrtc() {
        assert_eq!(VadAggressiveness::Quality.as_u8(), 0);
        assert_eq!(VadAggressiveness::VeryAggressive.as_u8(), 3);
    }

    #[test]
    fn vad_config_default_matches_spec() {
        let c = VadConfig::default();
        assert_eq!(c.aggressiveness, VadAggressiveness::Aggressive);
        assert_eq!(c.silence_hangover_frames, 25);
    }
}
