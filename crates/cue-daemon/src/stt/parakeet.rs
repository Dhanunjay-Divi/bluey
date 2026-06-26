//! On-device Parakeet STT provider — English streaming transcription (+ optional
//! on-demand Sortformer diarization) entirely on the user's machine, no network.
//!
//! `parakeet-rs` (NVIDIA Parakeet/Nemotron + Sortformer via ONNX Runtime) is a
//! SYNCHRONOUS, in-process inference library: `Nemotron::transcribe_chunk` runs
//! the model and returns the text. Our `SttProvider` trait, by contrast, is an
//! async connection model (`send_audio` is a fast non-blocking write; results
//! arrive separately via `next_event`). So this provider mirrors the proven
//! `LocalWhisperProvider` bridge: audio is pushed over an mpsc channel into a
//! dedicated blocking worker thread that runs inference, and the worker pushes
//! `TranscriptEvent`s back over a second channel that `next_event` awaits. The
//! CPU-heavy ONNX inference therefore never blocks the async runtime.
//!
//! Gated behind the `parakeet-stt` cargo feature so the heavy ONNX dependency is
//! opt-in and the default build is unaffected. See
//! docs/DECISION-VOICE-STT-STACK.md.

use std::path::PathBuf;

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};
use parakeet_rs::{Nemotron, NemotronMode};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// Parakeet model file locations the provider resolves at connect time.
#[derive(Debug, Clone)]
pub struct ParakeetPaths {
    /// Directory holding the Nemotron English ONNX model + tokenizer.
    pub nemotron_dir: PathBuf,
    /// Optional Sortformer diarization model. `None` disables diarization
    /// (transcription still works).
    pub sortformer_model: Option<PathBuf>,
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
/// emit events. Diarization runs on-demand alongside (see below).
fn run_worker(
    paths: ParakeetPaths,
    source: AudioSource,
    mut audio_rx: mpsc::UnboundedReceiver<Vec<f32>>,
    event_tx: mpsc::UnboundedSender<Result<TranscriptEvent, SttError>>,
) {
    // Load the English Nemotron model on this worker thread. `None` execution
    // config = default backend (native ONNX Runtime / load-dynamic per target).
    let mut nemotron = match Nemotron::from_pretrained(&paths.nemotron_dir, None) {
        Ok(mut n) => {
            // English-only is the model we ship (fastest, our use case).
            if n.mode() != NemotronMode::EnglishOnly {
                debug!("parakeet: model is not EnglishOnly mode ({:?})", n.mode());
            }
            n.reset();
            n
        }
        Err(e) => {
            let _ = event_tx.send(Err(SttError::Provider(format!(
                "failed to load Parakeet model from {}: {e}",
                paths.nemotron_dir.display()
            ))));
            return;
        }
    };

    // Optional on-demand diarization. Loaded lazily so transcription works even
    // when no diarization model is present.
    #[cfg(feature = "parakeet-stt")]
    let mut sortformer =
        paths.sortformer_model.as_ref().and_then(
            |p| match parakeet_rs::sortformer::Sortformer::new(p) {
                Ok(s) => Some(s),
                Err(e) => {
                    warn!("parakeet: diarization disabled (sortformer load failed): {e}");
                    None
                }
            },
        );

    // `blocking_recv` is correct here: this is a dedicated OS thread, not an
    // async task. The loop ends when the provider drops `audio_tx`.
    while let Some(chunk) = audio_rx.blocking_recv() {
        // STT: streaming, stateful — returns the incremental text for this chunk.
        match nemotron.transcribe_chunk(&chunk) {
            Ok(text) if !text.trim().is_empty() => {
                // parakeet-rs emits committed text per chunk; surface it as a
                // Final for the streamed text the meeting transcript consumes.
                if event_tx
                    .send(Ok(TranscriptEvent::Final {
                        text,
                        confidence: None,
                        source,
                        words: Vec::new(),
                    }))
                    .is_err()
                {
                    break; // consumer gone
                }
            }
            Ok(_) => {} // empty chunk, nothing to emit
            Err(e) => {
                error!("parakeet transcribe_chunk error: {e}");
                let _ = event_tx.send(Err(SttError::Provider(format!(
                    "parakeet transcription failed: {e}"
                ))));
                // Non-fatal: keep going on the next chunk.
            }
        }

        // Diarization on-demand: when enabled, label the speaker for this chunk.
        #[cfg(feature = "parakeet-stt")]
        if let Some(diar) = sortformer.as_mut() {
            match diar.diarize_chunk(&chunk) {
                Ok(segments) => {
                    if let Some(seg) = segments.last() {
                        let _ = event_tx.send(Ok(TranscriptEvent::SpeakerLabel {
                            speaker: seg.speaker_id as u32,
                            source,
                        }));
                    }
                }
                Err(e) => warn!("parakeet diarize_chunk error: {e}"),
            }
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
