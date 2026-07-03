//! Two-stage Voice Activity Detection.
//!
//! Stage 1 is an adaptive RMS gate: cheap, reject most obvious silence
//! before the more expensive WebRTC VAD sees it.
//!
//! Stage 2 is the WebRTC VAD (`webrtc-vad` crate): Chrome's production
//! neural VAD, runs on 10/20/30 ms frames at 8/16/32/48 kHz.
//!
//! Together they implement the [`cue_core::vad::FrameAction`] contract.

use anyhow::{bail, Result};
use cue_core::pcm::{AudioChunk, SampleRate};
use cue_core::vad::{FrameAction, VadAggressiveness, VadConfig};
use webrtc_vad::{SampleRate as WebRtcSampleRate, Vad, VadMode};

const DEFAULT_FRAME_MS: u32 = 20;

/// Adaptive RMS silence gate. Classifies an audio chunk as speech, trailing
/// silence (within hangover window), or drop (outside hangover window).
///
/// Threshold is expressed as a fraction of `i16::MAX`. Default starts at
/// 0.02 (~2% of full-scale) and slowly tracks the running noise floor so
/// quieter environments still admit speech.
pub struct RmsGate {
    /// Adaptive threshold in normalized [0.0, 1.0] units. Tracks the noise floor
    /// up AND down, clamped to never fall below `threshold_floor`.
    threshold: f32,
    /// Lower clamp for `threshold` — the configured start value. The threshold
    /// can rise above this to reject loud noise, but never drops below it, so a
    /// silent stream can't zero the gate out.
    threshold_floor: f32,
    /// Running noise-floor estimate (EMA of RMS during non-speech frames).
    noise_floor: f32,
    /// EMA coefficient for the noise-floor update.
    noise_alpha: f32,
    /// Max consecutive silence frames before moving Send → SendSilence → Drop.
    silence_hangover_limit: u32,
    /// Current silence-frame counter. 0 while speech is active.
    silence_count: u32,
}

impl RmsGate {
    pub fn new(config: &VadConfig) -> Self {
        Self {
            threshold: config.rms_threshold_start,
            threshold_floor: config.rms_threshold_start,
            noise_floor: 0.0,
            noise_alpha: 0.05,
            silence_hangover_limit: config.silence_hangover_frames,
            silence_count: 0,
        }
    }

    /// Classify a chunk. Also updates the adaptive noise-floor estimate.
    pub fn process(&mut self, chunk: &AudioChunk) -> FrameAction {
        let rms = normalized_rms(&chunk.samples);

        if rms >= self.threshold {
            // Speech-level energy. Reset silence counter.
            self.silence_count = 0;
            FrameAction::Send
        } else {
            // Below threshold — update noise floor (EMA) and re-track the
            // threshold to it. CRITICAL: the threshold must follow the noise floor
            // BOTH UP AND DOWN. An earlier version only ever RAISED the threshold
            // (`if adaptive > threshold`), which is a one-way ratchet: a loud
            // passage (music, applause) pushed it up, then quieter speech fell
            // below the stuck-high threshold and was dropped as "silence"
            // FOREVER — the transcript froze mid-sentence and never resumed even
            // though audio kept playing. We instead set the threshold to
            // `noise_floor * 3`, clamped to never drop below the configured start
            // (so it can't zero out on a very quiet stream). This lets the gate
            // relax again when the audio gets quieter.
            self.noise_floor = self.noise_floor * (1.0 - self.noise_alpha) + rms * self.noise_alpha;
            self.threshold = (self.noise_floor * 3.0).max(self.threshold_floor);

            self.silence_count = self.silence_count.saturating_add(1);
            if self.silence_count <= self.silence_hangover_limit {
                FrameAction::SendSilence
            } else {
                FrameAction::Drop
            }
        }
    }

    /// Current threshold (for test / debug visibility).
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Current noise floor estimate (for test / debug visibility).
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }
}

/// WebRTC VAD stage: a thin wrapper around the `webrtc-vad` crate that
/// validates frame-size + sample-rate compatibility up-front.
pub struct WebRtcGate {
    inner: Vad,
    sample_rate: SampleRate,
}

impl WebRtcGate {
    pub fn new(config: &VadConfig, sample_rate: SampleRate) -> Result<Self> {
        let wr_sr = match sample_rate.hz() {
            8_000 => WebRtcSampleRate::Rate8kHz,
            16_000 => WebRtcSampleRate::Rate16kHz,
            32_000 => WebRtcSampleRate::Rate32kHz,
            48_000 => WebRtcSampleRate::Rate48kHz,
            other => bail!(
                "WebRTC VAD requires 8/16/32/48 kHz sample rate, got {} Hz",
                other
            ),
        };

        let mode = match config.aggressiveness {
            VadAggressiveness::Quality => VadMode::Quality,
            VadAggressiveness::LowBitrate => VadMode::LowBitrate,
            VadAggressiveness::Aggressive => VadMode::Aggressive,
            VadAggressiveness::VeryAggressive => VadMode::VeryAggressive,
        };

        Ok(Self {
            inner: Vad::new_with_rate_and_mode(wr_sr, mode),
            sample_rate,
        })
    }

    /// `webrtc-vad` requires 10/20/30 ms frames at the configured rate.
    /// Given our default 20 ms Framer output, this always matches. We
    /// verify and propagate a useful error if the upstream ever changes.
    pub fn is_speech(&mut self, chunk: &AudioChunk) -> Result<bool> {
        if chunk.sample_rate != self.sample_rate {
            bail!(
                "sample-rate mismatch: gate configured for {} Hz but chunk is {} Hz",
                self.sample_rate.hz(),
                chunk.sample_rate.hz(),
            );
        }
        let duration_ms = chunk.duration_ms();
        if !matches!(duration_ms, 10 | 20 | 30) {
            bail!(
                "WebRTC VAD requires 10/20/30 ms frames; got {} ms ({} samples @ {} Hz)",
                duration_ms,
                chunk.samples.len(),
                self.sample_rate.hz(),
            );
        }
        self.inner
            .is_voice_segment(&chunk.samples)
            .map_err(|_| anyhow::anyhow!("webrtc-vad rejected frame"))
    }
}

/// The two-stage VAD pipeline. RMS gate first (cheap); if speech detected
/// or we're in the hangover window, ask the WebRTC VAD to confirm.
pub struct TwoStageVad {
    rms: RmsGate,
    webrtc: WebRtcGate,
}

impl TwoStageVad {
    pub fn new(config: &VadConfig, sample_rate: SampleRate) -> Result<Self> {
        Ok(Self {
            rms: RmsGate::new(config),
            webrtc: WebRtcGate::new(config, sample_rate)?,
        })
    }

    /// Run the chunk through both stages. Returns the final `FrameAction`.
    ///
    /// Decision matrix:
    /// - RMS says Drop → Drop (skip WebRTC)
    /// - RMS says Send → if WebRTC confirms speech: Send; otherwise SendSilence
    ///   (this avoids flagging sustained non-speech energy like a fan as speech)
    /// - RMS says SendSilence → SendSilence regardless (already in hangover)
    pub fn process(&mut self, chunk: &AudioChunk) -> FrameAction {
        match self.rms.process(chunk) {
            FrameAction::Drop => FrameAction::Drop,
            FrameAction::Send => match self.webrtc.is_speech(chunk) {
                Ok(true) => FrameAction::Send,
                Ok(false) => FrameAction::SendSilence,
                Err(_) => FrameAction::Send, // fail open — RMS already agreed
            },
            FrameAction::SendSilence => FrameAction::SendSilence,
        }
    }

    pub fn rms(&self) -> &RmsGate {
        &self.rms
    }
}

/// Runtime VAD config used by production capture loops.
///
/// Defaults are intentionally conservative: admit normal speech, forward a
/// short trailing-silence tail so STT providers can finalize utterances, then
/// drop sustained silence. Env overrides are for QA and field tuning only.
pub fn config_from_env() -> VadConfig {
    let default = VadConfig::default();
    let env = |names: &[&str]| -> Option<String> {
        names
            .iter()
            .find_map(|name| std::env::var(name).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    VadConfig {
        aggressiveness: env(&["BLUEY_VAD_AGGRESSIVENESS", "CUE_VAD_AGGRESSIVENESS"])
            .and_then(|value| parse_aggressiveness(&value))
            .unwrap_or(default.aggressiveness),
        rms_threshold_start: env(&["BLUEY_VAD_RMS_THRESHOLD", "CUE_VAD_RMS_THRESHOLD"])
            .and_then(|value| value.parse::<f32>().ok())
            .map(|value| value.clamp(0.001, 0.20))
            .unwrap_or(default.rms_threshold_start),
        silence_hangover_frames: env(&["BLUEY_VAD_HANGOVER_FRAMES", "CUE_VAD_HANGOVER_FRAMES"])
            .and_then(|value| value.parse::<u32>().ok())
            .map(|frames| frames.min(150))
            .or_else(|| {
                env(&["BLUEY_VAD_HANGOVER_MS", "CUE_VAD_HANGOVER_MS"])
                    .and_then(|value| value.parse::<u32>().ok())
                    .map(ms_to_frames)
            })
            .unwrap_or(default.silence_hangover_frames),
    }
}

fn parse_aggressiveness(value: &str) -> Option<VadAggressiveness> {
    match value.trim().to_ascii_lowercase().as_str() {
        "0" | "quality" => Some(VadAggressiveness::Quality),
        "1" | "low_bitrate" | "low-bitrate" | "lowbitrate" => Some(VadAggressiveness::LowBitrate),
        "2" | "aggressive" => Some(VadAggressiveness::Aggressive),
        "3" | "very_aggressive" | "very-aggressive" | "veryaggressive" => {
            Some(VadAggressiveness::VeryAggressive)
        }
        _ => None,
    }
}

fn ms_to_frames(ms: u32) -> u32 {
    ms.saturating_add(DEFAULT_FRAME_MS - 1)
        .saturating_div(DEFAULT_FRAME_MS)
        .min(150)
}

/// Normalized RMS in [0.0, 1.0]. Zero for empty input.
fn normalized_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    // Accumulate into i64 to avoid overflow on long chunks.
    let sum_sq: i64 = samples.iter().map(|&s| (s as i64) * (s as i64)).sum();
    let mean_sq = sum_sq as f64 / samples.len() as f64;
    let rms = mean_sq.sqrt();
    (rms / i16::MAX as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::pcm::AudioSource;

    fn chunk(samples: Vec<i16>) -> AudioChunk {
        AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(16_000).unwrap(),
            samples,
            captured_at_ms: 0,
        }
    }

    fn sine_chunk(amp: i16) -> AudioChunk {
        // 320 samples = 20 ms @ 16 kHz; simple triangle / ramp is enough for RMS.
        let samples: Vec<i16> = (0..320)
            .map(|i| {
                let v = (i % 32 - 16) * amp as i32 / 16;
                v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
            })
            .collect();
        chunk(samples)
    }

    #[test]
    fn normalized_rms_zero_for_silence() {
        assert_eq!(normalized_rms(&[0i16; 320]), 0.0);
    }

    #[test]
    fn normalized_rms_is_positive_for_signal() {
        let s: Vec<i16> = (0..320)
            .map(|i| if i % 2 == 0 { 10_000 } else { -10_000 })
            .collect();
        let r = normalized_rms(&s);
        assert!(r > 0.2 && r < 0.5);
    }

    #[test]
    fn rms_gate_admits_loud_audio() {
        let mut g = RmsGate::new(&VadConfig::default());
        let c = sine_chunk(30_000);
        assert_eq!(g.process(&c), FrameAction::Send);
        assert_eq!(g.process(&c), FrameAction::Send);
    }

    #[test]
    fn rms_gate_goes_send_silence_then_drop_on_sustained_silence() {
        let cfg = VadConfig {
            silence_hangover_frames: 3,
            ..VadConfig::default()
        };
        let mut g = RmsGate::new(&cfg);
        let loud = sine_chunk(30_000);
        g.process(&loud); // Send, clears silence counter

        let quiet = chunk(vec![0i16; 320]);
        assert_eq!(g.process(&quiet), FrameAction::SendSilence);
        assert_eq!(g.process(&quiet), FrameAction::SendSilence);
        assert_eq!(g.process(&quiet), FrameAction::SendSilence);
        // 4th consecutive silence frame exceeds hangover=3 → Drop
        assert_eq!(g.process(&quiet), FrameAction::Drop);
        assert_eq!(g.process(&quiet), FrameAction::Drop);
    }

    #[test]
    fn rms_gate_threshold_recovers_after_loud_passage() {
        // Regression: the gate used to RATCHET the threshold up only (a loud
        // passage raised it, then quieter speech fell below the stuck-high
        // threshold and was Dropped FOREVER — the transcript froze mid-sentence).
        // The threshold must come back DOWN once the audio quiets, so speech is
        // admitted again.
        let mut g = RmsGate::new(&VadConfig::default());
        let start = g.threshold();

        // To ratchet the threshold up we need frames whose RMS is BELOW the Send
        // threshold (so they take the noise-floor branch) but non-trivial, so the
        // noise floor — and thus `noise_floor * 3` — climbs above the start. A
        // constant-amplitude frame has RMS = amp/32768; pick amp so RMS ≈ 0.012
        // (below the 0.02 start, but ×3 = 0.036 > start). amp = 0.012 * 32768 ≈ 393.
        let below: i16 = 393;
        let mid = chunk(vec![below; 320]);
        assert!(
            normalized_rms(&vec![below; 320]) < start,
            "test frame must be below the Send threshold to exercise the ratchet"
        );
        for _ in 0..500 {
            g.process(&mid);
        }
        assert!(
            g.threshold() > start,
            "threshold should have risen above start ({start}); got {}",
            g.threshold()
        );

        // Now the audio goes quiet (near silence). The noise floor — and the
        // threshold — must DECAY back toward the floor, not stay latched high.
        let quiet = chunk(vec![0i16; 320]);
        for _ in 0..500 {
            g.process(&quiet);
        }
        let recovered = g.threshold();
        assert!(
            (recovered - start).abs() < 1e-4,
            "threshold must recover to its floor ({start}), got {recovered}"
        );

        // And real speech is admitted again — no permanent freeze.
        assert_eq!(g.process(&sine_chunk(30_000)), FrameAction::Send);
    }

    #[test]
    fn rms_gate_threshold_never_drops_below_floor() {
        // Even a fully-silent stream must not zero the gate out.
        let mut g = RmsGate::new(&VadConfig::default());
        let floor = g.threshold();
        let quiet = chunk(vec![0i16; 320]);
        for _ in 0..1000 {
            g.process(&quiet);
        }
        assert!(g.threshold() >= floor - 1e-6);
    }

    #[test]
    fn rms_gate_noise_floor_only_tracks_below_threshold_frames() {
        let mut g = RmsGate::new(&VadConfig::default());
        let loud = sine_chunk(30_000);
        let quiet = chunk(vec![50i16; 320]);

        g.process(&loud); // above threshold → no noise-floor update
        assert_eq!(g.noise_floor(), 0.0);
        g.process(&quiet); // below threshold → noise floor nudges up
        assert!(g.noise_floor() > 0.0);
    }

    #[test]
    fn webrtc_gate_rejects_non_standard_sample_rate() {
        let res = WebRtcGate::new(&VadConfig::default(), SampleRate::new(22_050).unwrap());
        assert!(res.is_err());
        assert!(format!("{:?}", res.err().unwrap()).contains("22050"));
    }

    #[test]
    fn webrtc_gate_accepts_standard_sample_rates() {
        for hz in [8_000, 16_000, 32_000, 48_000] {
            let sr = SampleRate::new(hz).unwrap();
            assert!(WebRtcGate::new(&VadConfig::default(), sr).is_ok());
        }
    }

    #[test]
    fn webrtc_gate_rejects_bad_frame_duration() {
        let mut g =
            WebRtcGate::new(&VadConfig::default(), SampleRate::new(16_000).unwrap()).unwrap();
        // 50 ms chunk — not 10/20/30
        let c = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(16_000).unwrap(),
            samples: vec![0i16; 800],
            captured_at_ms: 0,
        };
        let err = g.is_speech(&c).unwrap_err();
        assert!(format!("{:?}", err).contains("10/20/30"));
    }

    #[test]
    fn two_stage_vad_drops_silence() {
        let mut v = TwoStageVad::new(
            &VadConfig {
                silence_hangover_frames: 1,
                ..Default::default()
            },
            SampleRate::new(16_000).unwrap(),
        )
        .unwrap();
        let quiet = chunk(vec![0i16; 320]);
        // First silent chunk still in hangover (limit=1) → SendSilence
        assert_eq!(v.process(&quiet), FrameAction::SendSilence);
        // Second → Drop
        assert_eq!(v.process(&quiet), FrameAction::Drop);
    }

    #[test]
    fn two_stage_vad_forwards_loud_speech() {
        let mut v =
            TwoStageVad::new(&VadConfig::default(), SampleRate::new(16_000).unwrap()).unwrap();
        let loud = sine_chunk(30_000);
        // RMS admits; WebRTC may or may not confirm on synthetic triangle —
        // either Send or SendSilence is acceptable; just not Drop.
        let action = v.process(&loud);
        assert_ne!(action, FrameAction::Drop);
    }

    #[test]
    fn parses_aggressiveness_env_values() {
        assert_eq!(
            parse_aggressiveness("very-aggressive"),
            Some(VadAggressiveness::VeryAggressive)
        );
        assert_eq!(
            parse_aggressiveness("low_bitrate"),
            Some(VadAggressiveness::LowBitrate)
        );
        assert_eq!(parse_aggressiveness("wat"), None);
    }

    #[test]
    fn converts_hangover_ms_to_20ms_frames() {
        assert_eq!(ms_to_frames(0), 0);
        assert_eq!(ms_to_frames(1), 1);
        assert_eq!(ms_to_frames(500), 25);
        assert_eq!(ms_to_frames(501), 26);
        assert_eq!(ms_to_frames(10_000), 150);
    }
}
