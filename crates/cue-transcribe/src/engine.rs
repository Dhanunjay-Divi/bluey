//! Streaming ASR engine — NVIDIA Parakeet/Nemotron via parakeet-rs (ONNX).
//!
//! Cache-aware streaming: feed 16kHz mono f32 chunks; it emits incremental text
//! as ~560ms of audio accumulates. ONE engine instance (its own encoder/decoder
//! STATE) per audio source — the model is stateful, so mic and system audio must
//! not feed one engine (that corrupts both transcripts). The read-only WEIGHTS,
//! however, can be shared across sources: load a [`SttEngineHandle`] once and
//! spawn a per-source [`SttEngine::from_shared`], so two concurrent streams cost
//! one model in RAM. The speaker label comes from the SOURCE (mic = "You",
//! system = "They"); no diarization needed for v1.
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
    /// Hold-the-tip buffer (word-integrity fix): the last whitespace-delimited
    /// token of each chunk is PROVISIONAL — a word whose audio straddles the
    /// ~560ms chunk boundary is emitted half ("month") before the model has seen
    /// the rest ("ly"). We hold that trailing fragment back and prepend it to the
    /// next chunk's text, so "month" + "ly" reunite into "monthly" before either
    /// is committed. Flushed by `finalize_tip`. This is the local mitigation for
    /// the fully-causal encoder (no future right-context; see the crate notes).
    tip: String,
}

impl SttEngine {
    /// Load the Nemotron model from a directory containing `encoder.onnx`,
    /// `decoder_joint.onnx`, and `tokenizer.model`. Uses the CPU execution
    /// provider (the stable, fast default — see the crate docs). `coreml`/`cuda`
    /// cargo features can swap the provider, but CPU is default.
    ///
    /// This loads a DEDICATED copy of the model weights. When running multiple
    /// concurrent streams (e.g. mic and system audio), prefer the
    /// [`SttEngineHandle::load`] then [`SttEngine::from_shared`] path, which
    /// loads the ~650 MB weights ONCE and gives each stream its own decoder
    /// state — half the RAM, one load.
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        let exec = load_exec_config();
        let asr = Nemotron::from_pretrained(dir, Some(exec))
            .map_err(|e| anyhow!("failed to load Nemotron from {}: {e:?}", dir.display()))?;
        Ok(Self {
            asr,
            samples_seen: 0,
            tip: String::new(),
        })
    }

    /// Spawn a streaming engine that SHARES `handle`'s already-loaded model
    /// weights (the expensive ~650 MB ONNX session) while carrying its OWN
    /// encoder/decoder state. Two engines from the same handle transcribe two
    /// independent audio streams without interfering — the per-stream state is
    /// separate, only the read-only weights are shared. This is the correct way
    /// to run mic + system audio concurrently.
    pub fn from_shared(handle: &SttEngineHandle) -> Self {
        Self {
            asr: Nemotron::from_shared(&handle.inner),
            samples_seen: 0,
            tip: String::new(),
        }
    }
}

/// A loaded-once model handle whose read-only weights can back many concurrent
/// [`SttEngine`] streams via [`SttEngine::from_shared`]. Cheap to clone (it is
/// internally reference-counted). Load one per model directory, then spawn a
/// stream per audio source.
#[derive(Clone)]
pub struct SttEngineHandle {
    inner: parakeet_rs::NemotronHandle,
}

impl SttEngineHandle {
    /// Load the Nemotron model ONCE from a directory containing `encoder.onnx`,
    /// `decoder_joint.onnx`, and `tokenizer.model`. Subsequent
    /// [`SttEngine::from_shared`] calls reuse these weights.
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self> {
        let dir = model_dir.as_ref();
        let exec = load_exec_config();
        let inner = parakeet_rs::NemotronHandle::load(dir, Some(exec)).map_err(|e| {
            anyhow!(
                "failed to load Nemotron handle from {}: {e:?}",
                dir.display()
            )
        })?;
        Ok(Self { inner })
    }
}

/// The tuned CPU execution config (thread caps + spinning disabled). Both
/// `SttEngine::load` and `SttEngineHandle::load` build the model with this, so
/// the shared-weights path is byte-identical to the dedicated-load path.
///
/// CRITICAL: disable ONNX thread SPINNING + cap threads. ort's intra/inter op
/// thread pools busy-WAIT (spin) by default — faster ONLY for non-stop
/// inference, but for streaming audio (infrequent ~per-chunk work) the idle
/// threads burn CPU (measured: 330% CPU, 4 threads pegged) and STARVE the
/// blocking worker that pulls audio off the channel, so chunks pile up (measured
/// backlog 339→987 while push_ms stayed 0 — the model is instant, the spinning
/// pool was the bottleneck). ort's own docs: "spinning increases CPU usage,
/// disable it when use is infrequent." We disable spinning via the
/// SessionBuilder configure hook + cap threads.
fn load_exec_config() -> parakeet_rs::ExecutionConfig {
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
    ExecutionConfig::new()
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
        })
}

impl SttEngine {
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
        if text.is_empty() {
            return Ok(None);
        }

        // Hold-the-tip: reunite the previously-held fragment with this chunk's
        // text (keeping the model's own spacing), then split off the NEW trailing
        // token as the next tip. A word split across the boundary reassembles here
        // ("month" held + "ly" -> "monthly") before anything downstream sees it.
        let mut combined = std::mem::take(&mut self.tip);
        combined.push_str(&text);
        let emit = match combined.rfind(char::is_whitespace) {
            // Confirmed = everything up to (and incl.) the last space; hold the rest.
            Some(idx) => {
                let (confirmed, tip) = combined.split_at(idx + 1);
                self.tip = tip.to_string();
                confirmed.to_string()
            }
            // No space yet — the whole thing is still one unfinished word; keep holding.
            None => {
                self.tip = combined;
                String::new()
            }
        };
        if emit.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(TranscriptChunk { text: emit, at }))
    }

    /// Flush the held tip (the last provisional word). Call this when speech ends
    /// / on close, so a final word isn't stranded in the buffer. Returns the held
    /// text (may be empty).
    pub fn finalize_tip(&mut self) -> Option<TranscriptChunk> {
        let held = std::mem::take(&mut self.tip);
        if held.trim().is_empty() {
            return None;
        }
        let at = self.samples_seen as f64 / SAMPLE_RATE;
        Some(TranscriptChunk { text: held, at })
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
