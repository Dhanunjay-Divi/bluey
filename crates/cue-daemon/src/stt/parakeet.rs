//! On-device Parakeet STT provider — English streaming transcription (+ optional
//! on-demand Sortformer diarization) entirely on the user's machine, no network.
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
//! Diarization is NOT part of `cue-transcribe` (it omits it deliberately); the
//! on-demand Sortformer path below still uses `parakeet_rs::sortformer` directly.
//!
//! Gated behind the `parakeet-stt` cargo feature so the heavy ONNX dependency is
//! opt-in and the default build is unaffected. See
//! docs/DECISION-VOICE-STT-STACK.md.

use std::path::PathBuf;

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use self::assembler::{Emit, SentenceAssembler};

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
    // Load the streaming ASR engine on this worker thread. `cue-transcribe` owns
    // the Nemotron-wrapping inference (CPU execution provider by default) and
    // loads the model fresh, so the engine starts in a clean (reset) state.
    let mut engine = match cue_transcribe::SttEngine::load(&paths.nemotron_dir) {
        Ok(e) => e,
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

    // Sentence assembler (the production boundary recipe): buffers the engine's
    // raw per-chunk text and only emits a `Final` when a real sentence completes
    // (silence gate + punctuation + dangling-word check), coalescing short
    // fragments onto the prior sentence. The growing sentence streams as
    // `Partial` for live display. Without this, every ~560ms chunk became its own
    // `Final` and one spoken sentence fragmented into many segments.
    let mut assembler = SentenceAssembler::new();

    // `blocking_recv` is correct here: this is a dedicated OS thread, not an
    // async task. The loop ends when the provider drops `audio_tx`.
    while let Some(chunk) = audio_rx.blocking_recv() {
        // STT: streaming, stateful — returns the incremental text for this chunk.
        // `push` already returns `None` for empty/whitespace-only text, so we only
        // feed the assembler on `Some`. Measured RTF ~0.25 (250ms CPU per 1s of
        // audio) with diarization off, so the backlog stays at 0.
        //
        // DIAGNOSTIC: per-chunk RTF + backlog depth. If RTF ≥ 1.0 the worker is
        // over real-time and `backlog` climbs monotonically = the growing-lag bug.
        let audio_secs = chunk.len() as f64 / 16_000.0;
        // Silence signal for the assembler's boundary gate: RMS of this chunk.
        // Computed here (raw f32 available) BEFORE inference. NEVER used to gate
        // the engine feed — only to decide sentence boundaries downstream.
        let rms = if chunk.is_empty() {
            0.0
        } else {
            (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt()
        };
        let push_started = std::time::Instant::now();
        let backlog = audio_rx.len();
        let push_result = engine.push(&chunk);
        let rtf = push_started.elapsed().as_secs_f64() / audio_secs.max(1e-9);
        debug!(
            rtf = format!("{rtf:.2}"),
            audio_ms = (audio_secs * 1000.0) as u64,
            backlog,
            "STTPERF push"
        );
        let chunk_text = match push_result {
            Ok(Some(tc)) => Some(tc.text),
            Ok(None) => None,
            Err(e) => {
                error!("parakeet transcribe_chunk error: {e}");
                let _ = event_tx.send(Err(SttError::Provider(format!(
                    "parakeet transcription failed: {e}"
                ))));
                None // Non-fatal: keep going on the next chunk.
            }
        };

        // Feed the assembler: append any new text, advance the silence clock, and
        // emit Partial (growing sentence) + Final (completed sentence) events. A
        // single chunk can produce both (a partial update AND a boundary close).
        if let Some(text) = chunk_text {
            assembler.push_text(&text);
        }
        let audio_ms = (audio_secs * 1000.0) as u64;
        let mut consumer_gone = false;
        for emit in assembler.tick(rms, audio_ms) {
            let ev = match emit {
                Emit::Partial(text) => TranscriptEvent::Partial {
                    text,
                    confidence: None,
                    source,
                },
                Emit::Final(text) => TranscriptEvent::Final {
                    text,
                    confidence: None,
                    source,
                    words: Vec::new(),
                },
            };
            if event_tx.send(Ok(ev)).is_err() {
                consumer_gone = true;
                break;
            }
        }
        if consumer_gone {
            break;
        }

        // Diarization on-demand: when enabled, label the speaker for this chunk.
        // OFF by default (`sortformer` is `None` unless `BLUEY_STT_DIARIZE=1`) —
        // `diarize_chunk` is a second heavy ONNX model per chunk and, left on, it
        // pegs the worker so the audio backlog grows unboundedly (the multi-second
        // transcript lag). For system audio the speaker is always the source, so
        // this stays off. See `model_setup::parakeet_paths`.
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

/// The production sentence-boundary recipe, validated live on real meeting audio
/// (see docs/work/STT-DIARIZATION-FINDINGS.md). Buffers the engine's raw per-chunk
/// text and only closes a sentence on a real boundary, so one spoken sentence
/// stops fragmenting into many segments.
///
/// Boundary = a silence gate (`SILENCE_GATE_MS`) AND the buffer is not mid-thought
/// (ends on sentence punctuation, or does not end on a dangling function word),
/// OR a hard silence ceiling (`HARD_CEILING_MS`) so it never hangs. A too-short
/// fragment (< 2 content words, or only backchannel words) is coalesced onto the
/// previous sentence instead of standing alone — never dropped.
///
/// All boundary decisions are POST-model (on transcript text + a chunk RMS
/// silence signal). The engine feed is never gated (that desyncs the cache-aware
/// model — see the STT-feed invariants).
mod assembler {
    /// Emit ~500ms of silence to close a sentence (if it reads as complete).
    const SILENCE_GATE_MS: u64 = 500;
    /// Force-close after this much silence regardless (never hang).
    const HARD_CEILING_MS: u64 = 1_500;
    /// A chunk is "silent" below this RMS (fraction of full-scale).
    const RMS_SPEECH_THRESHOLD: f32 = 0.012;

    /// Words that mean the speaker is mid-thought — never end a sentence here.
    const DANGLING: &[&str] = &[
        "i", "the", "a", "an", "and", "or", "but", "to", "of", "in", "on", "for", "with", "at",
        "by", "was", "is", "are", "were", "um", "uh", "so", "that", "this", "we", "you", "it",
        "he", "she", "they", "my", "your", "our", "his", "her", "their", "some", "any", "as", "if",
        "when", "because", "would", "could", "should", "will", "can", "do", "did", "have", "has",
        "had",
    ];

    /// Short acknowledgments that attach to the prior sentence, never open one.
    const BACKCHANNEL: &[&str] = &[
        "yeah", "yep", "yup", "mhm", "mm", "uh-huh", "okay", "ok", "right", "sure", "no", "nope",
        "hmm", "yes", "cool", "nice",
    ];

    /// What the assembler wants the worker to emit this tick.
    pub enum Emit {
        /// The growing, still-open sentence (render live, replaces the prior partial).
        Partial(String),
        /// A completed sentence (committed transcript segment).
        Final(String),
    }

    pub struct SentenceAssembler {
        /// The current open sentence (raw text, model's own spacing preserved).
        buf: String,
        /// Consecutive silence, in audio-ms, since the last speech chunk.
        silent_ms: u64,
        /// The last completed sentence — a short fragment coalesces onto this.
        prev: String,
        /// Whether `buf` changed since the last Partial (to avoid dupe partials).
        dirty: bool,
    }

    impl SentenceAssembler {
        pub fn new() -> Self {
            Self {
                buf: String::new(),
                silent_ms: 0,
                prev: String::new(),
                dirty: false,
            }
        }

        /// Append newly-transcribed text to the open sentence.
        pub fn push_text(&mut self, text: &str) {
            if text.is_empty() {
                return;
            }
            self.buf.push_str(text);
            self.dirty = true;
        }

        /// Advance the silence clock by this chunk's audio, and decide whether to
        /// emit a Partial (buffer grew) and/or a Final (a boundary closed).
        pub fn tick(&mut self, rms: f32, audio_ms: u64) -> Vec<Emit> {
            if rms < RMS_SPEECH_THRESHOLD {
                self.silent_ms = self.silent_ms.saturating_add(audio_ms);
            } else {
                self.silent_ms = 0;
            }

            let mut out = Vec::new();

            // Live partial: emit the growing sentence when it changed.
            if self.dirty && !self.buf.trim().is_empty() {
                out.push(Emit::Partial(self.buf.trim().to_string()));
                self.dirty = false;
            }

            if let Some(sentence) = self.maybe_close() {
                out.push(Emit::Final(sentence));
            }
            out
        }

        /// Flush whatever is buffered as a Final (on close / end of speech).
        #[allow(dead_code)]
        pub fn flush(&mut self) -> Option<Emit> {
            let s = self.buf.trim().to_string();
            self.buf.clear();
            if s.is_empty() {
                None
            } else {
                self.prev = s.clone();
                Some(Emit::Final(s))
            }
        }

        /// The fused boundary gate. Returns Some(sentence) when the open buffer
        /// should close, applying the coalesce-short rule.
        fn maybe_close(&mut self) -> Option<String> {
            let trimmed = self.buf.trim();
            if trimmed.is_empty() {
                return None;
            }
            let ends_punct = trimmed
                .chars()
                .last()
                .map(|c| matches!(c, '.' | '?' | '!'))
                .unwrap_or(false);
            let last_word = word_key(trimmed.split_whitespace().last().unwrap_or(""));
            let dangling = DANGLING.contains(&last_word.as_str());

            let close = self.silent_ms >= HARD_CEILING_MS
                || (self.silent_ms >= SILENCE_GATE_MS && (ends_punct || !dangling));
            if !close {
                return None;
            }

            let sentence = trimmed.to_string();
            self.buf.clear();

            // Coalesce-not-drop: a short/backchannel-only fragment attaches to the
            // prior sentence (re-emit the merged prior) rather than standing alone.
            let words: Vec<&str> = sentence.split_whitespace().collect();
            let content_words = words
                .iter()
                .filter(|w| {
                    let k = word_key(w);
                    !k.is_empty() && !BACKCHANNEL.contains(&k.as_str())
                })
                .count();
            let all_backchannel = words.iter().all(|w| {
                let k = word_key(w);
                k.is_empty() || BACKCHANNEL.contains(&k.as_str())
            });

            if (content_words < 2 || all_backchannel) && !self.prev.is_empty() {
                self.prev.push(' ');
                self.prev.push_str(&sentence);
                return Some(self.prev.clone());
            }

            self.prev = sentence.clone();
            Some(sentence)
        }
    }

    /// Normalize a token to its lowercase alphanumeric core for matching.
    fn word_key(w: &str) -> String {
        w.trim_matches(|c: char| !c.is_alphanumeric())
            .to_ascii_lowercase()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn buffers_chunks_until_silence_then_finalizes() {
            let mut a = SentenceAssembler::new();
            a.push_text("are there any ");
            a.push_text("other changes from staff");
            // speech ongoing → no final
            assert!(a
                .tick(0.2, 560)
                .iter()
                .all(|e| matches!(e, Emit::Partial(_))));
            // silence accrues past the gate; sentence doesn't end on a dangling word.
            let mut got_final = None;
            for _ in 0..3 {
                for e in a.tick(0.0, 200) {
                    if let Emit::Final(s) = e {
                        got_final = Some(s);
                    }
                }
            }
            assert_eq!(
                got_final.as_deref(),
                Some("are there any other changes from staff")
            );
        }

        #[test]
        fn dangling_word_keeps_buffering_through_a_pause() {
            let mut a = SentenceAssembler::new();
            a.push_text("we should ship this because"); // ends on "because" (dangling)
            let mut closed = false;
            for _ in 0..3 {
                for e in a.tick(0.0, 200) {
                    if matches!(e, Emit::Final(_)) {
                        closed = true;
                    }
                }
            }
            assert!(!closed, "must not close mid-thought on a dangling word");
        }

        #[test]
        fn short_fragment_coalesces_onto_prior() {
            let mut a = SentenceAssembler::new();
            a.push_text("let's go with vulnerability management");
            // close the first sentence
            for _ in 0..8 {
                let _ = a.tick(0.0, 200);
            }
            // a lone "yeah" arrives and pauses → should attach, not stand alone
            a.push_text("yeah");
            let mut last_final = None;
            for _ in 0..8 {
                for e in a.tick(0.0, 200) {
                    if let Emit::Final(s) = e {
                        last_final = Some(s);
                    }
                }
            }
            assert_eq!(
                last_final.as_deref(),
                Some("let's go with vulnerability management yeah")
            );
        }

        #[test]
        fn hard_ceiling_closes_even_a_dangling_buffer() {
            let mut a = SentenceAssembler::new();
            a.push_text("and then we"); // ends dangling on "we"
            let mut closed = None;
            for _ in 0..10 {
                for e in a.tick(0.0, 200) {
                    if let Emit::Final(s) = e {
                        closed = Some(s);
                    }
                }
            }
            assert_eq!(closed.as_deref(), Some("and then we"));
        }
    }
}
