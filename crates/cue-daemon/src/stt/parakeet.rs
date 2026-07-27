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
use std::sync::{Condvar, Mutex, OnceLock};

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};
use tokio::sync::mpsc;
use tracing::{debug, error};

enum SharedModelState<T> {
    Empty,
    Loading,
    Ready { dir: PathBuf, value: T },
}

/// A blocking, process-local single-flight cache for expensive model loads.
///
/// The loader runs without holding the mutex. Concurrent microphone/system
/// workers wait on the condition variable and then clone the one finished
/// handle, instead of loading two ~650 MB copies at once.
struct SharedModelCache<T> {
    state: Mutex<SharedModelState<T>>,
    ready: Condvar,
}

impl<T> Default for SharedModelCache<T> {
    fn default() -> Self {
        Self {
            state: Mutex::new(SharedModelState::Empty),
            ready: Condvar::new(),
        }
    }
}

impl<T: Clone> SharedModelCache<T> {
    fn get_or_try_init<E>(
        &self,
        dir: &Path,
        load: impl FnOnce(&Path) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut load = Some(load);
        loop {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            match &*state {
                SharedModelState::Ready {
                    dir: cached_dir,
                    value,
                } if cached_dir == dir => return Ok(value.clone()),
                SharedModelState::Loading => {
                    state = self
                        .ready
                        .wait(state)
                        .unwrap_or_else(|poison| poison.into_inner());
                    drop(state);
                }
                SharedModelState::Empty | SharedModelState::Ready { .. } => {
                    *state = SharedModelState::Loading;
                    drop(state);

                    let result = load
                        .take()
                        .expect("the cache loader is consumed only by the winning caller")(
                        dir
                    );
                    let mut state = self
                        .state
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner());
                    match &result {
                        Ok(value) => {
                            *state = SharedModelState::Ready {
                                dir: dir.to_path_buf(),
                                value: value.clone(),
                            };
                        }
                        Err(_) => *state = SharedModelState::Empty,
                    }
                    self.ready.notify_all();
                    return result;
                }
            }
        }
    }
}

/// Process-wide shared Nemotron weights (backed by an `Arc` inside
/// `SttEngineHandle`). The first load also runs a disposable silent encoder
/// window, so later real streams reuse both the weights and the initialized ONNX
/// execution path.
static SHARED_STT_HANDLE: OnceLock<SharedModelCache<cue_transcribe::SttEngineHandle>> =
    OnceLock::new();

fn shared_handle_for(dir: &Path) -> anyhow::Result<cue_transcribe::SttEngineHandle> {
    SHARED_STT_HANDLE
        .get_or_init(SharedModelCache::default)
        .get_or_try_init(dir, |model_dir| {
            let load_started = std::time::Instant::now();
            let handle = cue_transcribe::SttEngineHandle::load(model_dir)?;
            let load_ms = load_started.elapsed().as_millis();

            let warm_started = std::time::Instant::now();
            handle.warm_up()?;
            tracing::info!(
                dir = %model_dir.display(),
                load_ms,
                warm_ms = warm_started.elapsed().as_millis(),
                "Parakeet model loaded and inference path prewarmed"
            );
            Ok(handle)
        })
}

/// Load and prewarm the process-wide Parakeet model before capture begins.
///
/// Safe to call repeatedly and concurrently. Once ready, this is only a cheap
/// handle clone; a simultaneous source startup waits for the same single-flight
/// load rather than allocating duplicate model weights.
pub fn prewarm(paths: &ParakeetPaths) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let _ = shared_handle_for(&paths.nemotron_dir)?;
    tracing::info!(
        dir = %paths.nemotron_dir.display(),
        elapsed_ms = started.elapsed().as_millis(),
        "Parakeet STT prewarm ready"
    );
    Ok(())
}

/// Get a clean streaming engine for `dir`, sharing the process-wide prewarmed
/// weights with every other source.
fn shared_engine_for(dir: &Path) -> anyhow::Result<cue_transcribe::SttEngine> {
    let handle = shared_handle_for(dir)?;
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
        let push_elapsed = push_started.elapsed();
        let push_ms = push_elapsed.as_millis();
        let rtf = push_elapsed.as_secs_f64() / audio_secs.max(1e-9);

        let chunk_text = match push_result {
            Ok(Some(tc)) => {
                let text = tc.text.trim();
                if !text.is_empty() {
                    has_spoken_since_last_boundary = true;
                    // Keep latency telemetry off stderr: eprintln! synchronously
                    // locks and writes on the inference thread, adding avoidable
                    // work to the hot path. It also leaked meeting text into the
                    // terminal. Debug fields retain the useful timing/backlog
                    // signal without recording transcript content.
                    debug!(
                        ?source,
                        push_ms,
                        rtf,
                        backlog,
                        emitted_chars = tc.text.len(),
                        "Parakeet streaming push"
                    );
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
                None
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

#[cfg(test)]
mod tests {
    use super::SharedModelCache;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    #[test]
    fn concurrent_sources_share_one_model_load() {
        const WORKERS: usize = 8;
        let cache = Arc::new(SharedModelCache::<usize>::default());
        let starts = Arc::new(Barrier::new(WORKERS));
        let loads = Arc::new(AtomicUsize::new(0));

        let workers = (0..WORKERS)
            .map(|_| {
                let cache = cache.clone();
                let starts = starts.clone();
                let loads = loads.clone();
                std::thread::spawn(move || {
                    starts.wait();
                    cache
                        .get_or_try_init(Path::new("/test/model"), |_| {
                            loads.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(25));
                            Ok::<usize, ()>(42)
                        })
                        .expect("single-flight load")
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            assert_eq!(worker.join().expect("worker join"), 42);
        }
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_model_load_unblocks_a_later_retry() {
        let cache = SharedModelCache::<usize>::default();
        let first = cache.get_or_try_init(Path::new("/test/model"), |_| {
            Err::<usize, &'static str>("load failed")
        });
        assert_eq!(first, Err("load failed"));

        let retried = cache
            .get_or_try_init(Path::new("/test/model"), |_| Ok::<usize, &'static str>(7))
            .expect("retry after failed load");
        assert_eq!(retried, 7);
    }

    #[test]
    fn a_different_model_directory_gets_its_own_load() {
        let cache = SharedModelCache::<usize>::default();
        let loads = AtomicUsize::new(0);

        let first = cache
            .get_or_try_init(Path::new("/test/model-a"), |_| {
                loads.fetch_add(1, Ordering::SeqCst);
                Ok::<usize, ()>(1)
            })
            .expect("first model");
        let second = cache
            .get_or_try_init(Path::new("/test/model-b"), |_| {
                loads.fetch_add(1, Ordering::SeqCst);
                Ok::<usize, ()>(2)
            })
            .expect("second model");

        assert_eq!((first, second), (1, 2));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }
}
