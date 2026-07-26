//! On-device Parakeet STT provider — English streaming transcription entirely on
//! the user's machine, no network.
//!
//! ASR is delegated to the shared `cue-transcribe` crate ([`cue_transcribe::SttEngine`]),
//! the single home for the Nemotron-wrapping inference code. The engine is a
//! SYNCHRONOUS, in-process streaming model: `SttEngine::push` runs the model and
//! returns any newly committed text. Our `SttProvider` trait, by contrast, is an
//! async connection model (`send_audio` is a fast non-blocking write; results
//! arrive separately via `next_event`). So this provider mirrors the proven
//! `LocalWhisperProvider` bridge: audio is pushed over an mpsc channel into a
//! dedicated blocking worker thread that runs inference, and the worker pushes
//! `TranscriptEvent`s back over a second channel that `next_event` awaits. The
//! CPU-heavy ONNX inference therefore never blocks the async runtime.
//!
//! Speaker diarization is NOT done here: the live speaker labels come entirely
//! from `cue-diarize` (speakrs). The old STT-path Sortformer diarizer was removed
//! because its per-chunk output was discarded downstream.
//!
//! Gated behind the `parakeet-stt` cargo feature so the heavy ONNX dependency is
//! opt-in and the default build is unaffected. See
//! docs/DECISION-VOICE-STT-STACK.md.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};
use tokio::sync::mpsc;
use tracing::{debug, error};

/// Process-wide shared Nemotron weights (loaded once, backed by an `Arc` inside
/// `SttEngineHandle`). The first STT source to start pays the ~650 MB load; every
/// later source spawns a `from_shared` engine reusing these weights, so mic +
/// system audio cost ONE model in RAM. Keyed by model dir so a different model
/// path (rare) loads its own handle rather than mis-sharing.
static SHARED_STT_HANDLE: OnceLock<Mutex<Option<(PathBuf, cue_transcribe::SttEngineHandle)>>> =
    OnceLock::new();

/// Get a streaming engine for `dir`, sharing weights with any engine already
/// loaded for the same dir. Loads the handle on first use (per dir). Falls back
/// to a dedicated `SttEngine::load` only if the handle can't be built (so a
/// single source is never blocked by the sharing machinery).
fn shared_engine_for(dir: &Path) -> anyhow::Result<cue_transcribe::SttEngine> {
    let slot = SHARED_STT_HANDLE.get_or_init(|| Mutex::new(None));

    // FAST PATH: take the lock, check the cache, RELEASE it. Never hold the lock
    // across the load.
    {
        let guard = slot.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((cached_dir, h)) = guard.as_ref() {
            if cached_dir == dir {
                return Ok(cue_transcribe::SttEngine::from_shared(h));
            }
        }
    }

    // SLOW PATH: load OUTSIDE the lock.
    //
    // DEADLOCK FIX (2026-07-20, found live on an Intel Mac): this previously
    // held the mutex across `SttEngineHandle::load` — a ~650 MB, multi-second
    // blocking load. Mic and system audio start at the SAME instant, so source
    // A took the lock and began loading while source B blocked on it. Both
    // threads sat in `__psynch_mutexwait` and NEITHER ever reached the code that
    // logs success or failure: the daemon log dead-ended at "parakeet model
    // already present", RSS stayed at 31 MB (model never resident), and
    // transcription silently never started. Loading off-lock means a concurrent
    // start can never block on a load.
    //
    // Cost of the race: if both sources miss the cache simultaneously they may
    // each load once, and the last writer wins. That is a bounded, one-time
    // duplicate load — strictly better than a hang, and the steady state is
    // still one shared handle.
    let handle = cue_transcribe::SttEngineHandle::load(dir)?;

    {
        let mut guard = slot.lock().unwrap_or_else(|p| p.into_inner());
        match guard.as_ref() {
            // Someone else finished first — prefer THEIR handle so both sources
            // converge on one set of weights.
            Some((cached_dir, existing)) if cached_dir == dir => {
                return Ok(cue_transcribe::SttEngine::from_shared(existing));
            }
            _ => *guard = Some((dir.to_path_buf(), handle.clone())),
        }
    }
    Ok(cue_transcribe::SttEngine::from_shared(&handle))
}

/// Parakeet model file locations the provider resolves at connect time.
#[derive(Debug, Clone)]
pub struct ParakeetPaths {
    /// Directory holding the Nemotron English ONNX model + tokenizer.
    pub nemotron_dir: PathBuf,
}

/// On-device English STT provider backed by `parakeet-rs`.
pub struct ParakeetProvider {
    state: ConnectionState,
    /// Audio in → worker. Dropped on `close`, which ends the worker loop.
    audio_tx: Option<mpsc::UnboundedSender<Vec<f32>>>,
    /// Transcript events out ← worker.
    event_rx: mpsc::UnboundedReceiver<Result<TranscriptEvent, SttError>>,
    /// The blocking inference worker; joined on drop via the channel close.
    _worker: Option<std::thread::JoinHandle<()>>,
}

impl ParakeetProvider {
    /// Load the model(s) and start the inference worker. The ONNX model is
    /// loaded ON the worker thread (it's heavy and `!Send`-friendly to keep
    /// local), so this returns quickly and surfaces a load failure as the first
    /// event rather than blocking the caller.
    pub fn connect(paths: ParakeetPaths, source: AudioSource) -> Self {
        let (audio_tx, audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Dedicated OS thread (not a tokio task): ONNX inference is blocking and
        // CPU-bound; keeping it off the async runtime avoids starving it.
        let worker = std::thread::Builder::new()
            .name("parakeet-stt".into())
            .spawn(move || run_worker(paths, source, audio_rx, event_tx))
            .expect("spawn parakeet worker thread");

        Self {
            state: ConnectionState::Connected,
            audio_tx: Some(audio_tx),
            event_rx,
            _worker: Some(worker),
        }
    }
}

/// The blocking inference loop: load the model, then transcribe each chunk and
/// emit events.
fn run_worker(
    paths: ParakeetPaths,
    source: AudioSource,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<f32>>,
    event_tx: mpsc::UnboundedSender<Result<TranscriptEvent, SttError>>,
) {
    // Load the streaming ASR engine on this worker thread, SHARING the model
    // weights across sources. The first source to start loads the ~650 MB
    // Nemotron weights once into a process-wide handle; every subsequent source
    // (e.g. the microphone alongside system audio) spawns a `from_shared` engine
    // with its OWN decoder state but the SAME read-only weights — so two
    // concurrent sources cost one model in RAM, not two. Each engine still starts
    // in a clean (reset) state, and the streams never interfere.
    let mut engine = match shared_engine_for(&paths.nemotron_dir) {
        Ok(e) => {
            tracing::info!(
                source = ?source,
                dir = %paths.nemotron_dir.display(),
                "STT engine ready"
            );
            e
        }
        Err(e) => {
            // LOG it, don't only send it. This error previously went ONLY into
            // the event channel, so a failed engine load was completely silent
            // in the daemon log: capture said "started", nothing transcribed,
            // and there was no error anywhere to explain why. Cost hours of
            // blind debugging on a real Intel Mac (2026-07-20).
            tracing::error!(
                source = ?source,
                dir = %paths.nemotron_dir.display(),
                error = %e,
                "failed to load Parakeet STT engine — transcription will not work"
            );
            let _ = event_tx.send(Err(SttError::Provider(format!(
                "failed to load Parakeet model from {}: {e}",
                paths.nemotron_dir.display()
            ))));
            return;
        }
    };

    // Direct emission: each engine delta becomes one `Final`. The hold-the-tip
    // logic in cue-transcribe already reunites words split across chunk
    // boundaries, so downstream fragments append cleanly with no duplication.
    // (A sentence-assembler that buffered + re-emitted a growing sentence lived
    // here briefly but caused cumulative-duplication; see the emit site below.)

    // `blocking_recv` is correct here: this is a dedicated OS thread, not an
    // async task. The loop ends when the provider drops `audio_tx`.
    let mut silent_ms = 0.0;
    let mut has_spoken_since_last_boundary = false;
    const SILENCE_GATE_MS: f64 = 250.0;
    const RMS_THRESHOLD: f32 = 0.01;

    while let Some(chunk) = audio_rx.blocking_recv() {
        // STT: streaming, stateful — returns the incremental text for this chunk.
        // `push` already returns `None` for empty/whitespace-only text, so we only
        // feed the assembler on `Some`. Measured RTF ~0.25 (250ms CPU per 1s of
        // audio) with diarization off, so the backlog stays at 0.
        //
        // DIAGNOSTIC: per-chunk RTF + backlog depth. If RTF ≥ 1.0 the worker is
        // over real-time and `backlog` climbs monotonically = the growing-lag bug.
        let audio_secs = chunk.len() as f64 / 16_000.0;
        let audio_ms = audio_secs * 1000.0;

        // Silence signal for the assembler's boundary gate: RMS of this chunk.
        // Computed here (raw f32 available) BEFORE inference.
        let rms = if chunk.is_empty() {
            0.0
        } else {
            (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt()
        };

        if rms < RMS_THRESHOLD {
            silent_ms += audio_ms;
        } else {
            silent_ms = 0.0;
        }

        let push_started = std::time::Instant::now();
        let backlog = audio_rx.len();
        let push_result = engine.push(&chunk);
        let rtf = push_started.elapsed().as_secs_f64() / audio_secs.max(1e-9);
        debug!(
            rtf = format!("{rtf:.2}"),
            audio_ms = audio_ms as u64,
            backlog,
            "STTPERF push"
        );
        
        let chunk_text = match push_result {
            Ok(Some(tc)) => {
                let text = tc.text.trim();
                if !text.is_empty() {
                    has_spoken_since_last_boundary = true;
                    Some(tc.text)
                } else {
                    None
                }
            }
            Ok(None) => None,
            Err(e) => {
                error!("parakeet transcribe_chunk error: {e}");
                let _ = event_tx.send(Err(SttError::Provider(format!(
                    "parakeet transcription failed: {e}"
                ))));
                None // Non-fatal: keep going on the next chunk.
            }
        };

        // Emit each engine delta DIRECTLY as a Final, exactly as it comes from
        // hold-the-tip (clean incremental text: "I ", "want ", "ultimately ").
        // This is the proven, duplication-free path: the engine already reunites
        // split words, so each delta is a whole new fragment that simply appends.
        if let Some(text) = chunk_text {
            let ev = TranscriptEvent::Final {
                text,
                confidence: None,
                source,
                words: Vec::new(),
            };
            if event_tx.send(Ok(ev)).is_err() {
                break;
            }
        }

        // Emit Boundary if we've crossed the silence threshold AND we have 
        // transcribed speech since the last boundary.
        if silent_ms >= SILENCE_GATE_MS && has_spoken_since_last_boundary {
            let ev = TranscriptEvent::Boundary { source };
            if event_tx.send(Ok(ev)).is_err() {
                break;
            }
            has_spoken_since_last_boundary = false;
            silent_ms = 0.0;
        }
    }

    debug!("parakeet worker loop ended (audio channel closed)");
}

#[async_trait]
impl SttProvider for ParakeetProvider {
    fn name(&self) -> &'static str {
        "parakeet_nemotron_en"
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        // parakeet-rs wants 16kHz mono f32 in [-1.0, 1.0]; our AudioChunk carries
        // PCM16 (`i16`) mono. Convert here (i16 / 32768.0). Resampling to 16k, if
        // the source rate differs, is handled upstream in the capture pipeline.
        let samples: Vec<f32> = chunk.samples.iter().map(|&s| s as f32 / 32768.0).collect();
        match self.audio_tx.as_ref() {
            Some(tx) => tx
                .send(samples)
                .map_err(|_| SttError::Provider("parakeet worker is gone".into())),
            None => Err(SttError::Provider("parakeet provider is closed".into())),
        }
    }

    async fn finalize(&self) -> Result<(), SttError> {
        // Streaming chunk model: each chunk is already committed, so there is no
        // separate "force final" step. Safe no-op.
        Ok(())
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        self.event_rx.recv().await
    }

    async fn close(&mut self) -> Result<(), SttError> {
        // Dropping the sender ends the worker's recv loop; the thread finishes.
        self.audio_tx = None;
        self.state = ConnectionState::Closed;
        Ok(())
    }
}
