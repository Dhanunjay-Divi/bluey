//! Streaming ASR engine — NVIDIA Parakeet/Nemotron via parakeet-rs (ONNX).
//!
//! Cache-aware streaming: feed 16kHz mono f32 chunks; it emits incremental text
//! as ~560ms of audio accumulates. ONE engine instance per audio source — the
//! model is stateful, so mic and system audio must NOT share one instance (that
//! corrupts both transcripts). The speaker label comes from the SOURCE (mic =
//! "You", system = "They"); no diarization needed for v1.
//!
//! Adapted from the proven `nifty-brahmagupta` prototype, leaned down to just the
//! ASR path (the prototype's Sortformer diarization — the slow 90%-compute part —
//! is intentionally omitted here).

use std::path::Path;

use anyhow::{anyhow, Result};
use parakeet_rs::Nemotron;

const SAMPLE_RATE: f64 = 16_000.0;

/// One streaming ASR engine, bound to a single audio source. Stateful — do not
/// share across sources.
pub struct SttEngine {
    asr: Nemotron,
    samples_seen: usize,
}

impl SttEngine {
    /// Load the Nemotron model from a directory containing `encoder.onnx`,
    /// `decoder_joint.onnx`, and `tokenizer.model`. Uses the CPU execution
    /// provider (the stable, fast default — see the crate docs). `coreml`/`cuda`
    /// cargo features can swap the provider, but CPU is default.
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        // CPU execution (CoreML is unstable for this model; CPU is faster anyway).
        //
        // CRITICAL: disable ONNX thread SPINNING + cap threads. ort's intra/inter
        // op thread pools busy-WAIT (spin) by default — which is faster ONLY for
        // non-stop inference, but for streaming audio (infrequent ~per-chunk work)
        // the idle threads burn CPU (measured: 330% CPU, 4 threads pegged) and
        // STARVE the blocking worker that pulls audio off the channel, so chunks
        // pile up (measured backlog 339→987 while push_ms stayed 0 — the model is
        // instant, the spinning pool was the bottleneck). ort's own docs:
        // "spinning increases CPU usage, disable it when use is infrequent."
        // We disable spinning via the SessionBuilder configure hook + cap threads.
        use parakeet_rs::ExecutionConfig;
        // Intra-op threads default to 2 (measured sweet spot: RTF ~0.25 on Apple
        // Silicon, no spinning). Overridable via `BLUEY_STT_INTRA_THREADS` so a
        // many-core machine can go faster or a constrained one can pin to 1 — the
        // model runs CPU-only, so this is the main lever for weaker laptops.
        let intra_threads = std::env::var("BLUEY_STT_INTRA_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n >= 1)
            .unwrap_or(2);
        let exec = ExecutionConfig::new()
            .with_intra_threads(intra_threads)
            .with_inter_threads(1)
            .with_custom_configure(|builder| {
                // with_*_spinning returns Result<SessionBuilder, Error<SessionBuilder>>;
                // the hook wants ort::Result<SessionBuilder> (Error<()>). Re-wrap the
                // error message into the plain error type to bridge them.
                let builder = builder
                    .with_intra_op_spinning(false)
                    .map_err(|e| ort::Error::new(e.message()))?;
                builder
                    .with_inter_op_spinning(false)
                    .map_err(|e| ort::Error::new(e.message()))
            });
        let asr = Nemotron::from_pretrained(dir, Some(exec))
            .map_err(|e| anyhow!("failed to load Nemotron from {}: {e:?}", dir.display()))?;
        Ok(Self {
            asr,
            samples_seen: 0,
        })
    }

    /// Feed one chunk of 16kHz mono f32 samples (typically ~100ms). Returns any
    /// newly transcribed text — possibly empty while the model accumulates its
    /// ~560ms window. The text is returned RAW: Nemotron encodes word boundaries
    /// in its own leading/trailing spaces, so callers must NOT trim+re-space it
    /// (that splits words spanning chunk boundaries).
    pub fn push(&mut self, pcm: &[f32]) -> Result<Option<TranscriptChunk>> {
        self.samples_seen += pcm.len();
        let at = self.samples_seen as f64 / SAMPLE_RATE;
        let text = self
            .asr
            .transcribe_chunk(pcm)
            .map_err(|e| anyhow!("Nemotron transcribe_chunk failed: {e:?}"))?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(TranscriptChunk { text, at }))
    }

    /// The full accumulated transcript so far.
    pub fn full_transcript(&self) -> String {
        self.asr.get_transcript()
    }
}

/// A piece of streamed transcript text plus the stream time (seconds) it landed.
#[derive(Debug, Clone)]
pub struct TranscriptChunk {
    /// Raw incremental text (keep the model's own spacing — do not trim).
    pub text: String,
    /// Stream time in seconds at which this text was emitted.
    pub at: f64,
}
