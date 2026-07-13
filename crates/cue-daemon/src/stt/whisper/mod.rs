//! Local Whisper STT provider — spawns a native helper binary as a child process.
//!
//! The helper reads PCM16 LE 16kHz mono from stdin and emits NDJSON transcript
//! events on stdout. This provider implements the standard SttProvider trait.
//!
//! Binary resolution order:
//! 1. `BLUEY_LOCAL_WHISPER_BINARY` env var (for tests)
//! 2. Platform-specific default path

pub mod error;
pub mod parser;

use async_trait::async_trait;
use cue_core::pcm::AudioChunk;
use cue_core::stt::{ConnectionState, SttConfig, SttError, SttProvider, TranscriptEvent};
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tracing::{debug, error, warn};

use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use self::error::WhisperError;
use self::parser::parse_line;
use crate::audio::capture::{latest_channel, LatestReceiver, LatestSendResult, LatestSender};

const AUDIO_QUEUE_CAPACITY: usize = 50;
const EVENT_QUEUE_CAPACITY: usize = 64;
const HELPER_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// Local Whisper STT provider using a child-process helper binary.
pub struct LocalWhisperProvider {
    state: ConnectionState,
    event_rx: LatestReceiver<Result<TranscriptEvent, SttError>>,
    stdin_tx: Option<LatestSender<Vec<u8>>>,
    child_handle: Option<tokio::task::JoinHandle<()>>,
    dropped_audio_chunks: AtomicU64,
    dropped_partial_events: Arc<AtomicU64>,
}

impl LocalWhisperProvider {
    /// Create and start a new LocalWhisperProvider.
    pub fn connect(config: SttConfig) -> Result<Self, WhisperError> {
        let binary = resolve_binary()?;
        let (event_tx, event_rx) = latest_channel(EVENT_QUEUE_CAPACITY);
        let (stdin_tx, stdin_rx) = latest_channel::<Vec<u8>>(AUDIO_QUEUE_CAPACITY);
        let dropped_partial_events = Arc::new(AtomicU64::new(0));

        let source = config.source;
        let task_dropped_partial_events = dropped_partial_events.clone();
        let handle = tokio::spawn(async move {
            run_helper_loop(
                binary,
                source,
                event_tx,
                stdin_rx,
                task_dropped_partial_events,
            )
            .await;
        });

        Ok(Self {
            state: ConnectionState::Connected,
            event_rx,
            stdin_tx: Some(stdin_tx),
            child_handle: Some(handle),
            dropped_audio_chunks: AtomicU64::new(0),
            dropped_partial_events,
        })
    }

    pub fn dropped_audio_chunks(&self) -> u64 {
        self.dropped_audio_chunks.load(Ordering::Relaxed)
    }

    pub fn dropped_partial_events(&self) -> u64 {
        self.dropped_partial_events.load(Ordering::Relaxed)
    }
}

/// Resolve the helper binary path.
fn resolve_binary() -> Result<String, WhisperError> {
    // 1. Env override (for tests)
    if let Ok(path) = std::env::var("BLUEY_LOCAL_WHISPER_BINARY") {
        return Ok(path);
    }

    // 2. Platform default
    #[cfg(target_os = "macos")]
    {
        let mut candidates = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            let mut dirs = Vec::new();
            if let Some(dir) = exe.parent() {
                dirs.push(dir.to_path_buf());
            }
            if let Ok(canonical) = exe.canonicalize() {
                if let Some(dir) = canonical.parent() {
                    dirs.push(dir.to_path_buf());
                }
            }
            for dir in dirs {
                candidates.extend([
                    dir.join("cue-whisper"),
                    dir.join("bluey-whisper-macos"),
                    dir.join("bin/cue-whisper"),
                    dir.join("bin/bluey-whisper-macos"),
                ]);
            }
        }
        candidates.extend([
            std::path::PathBuf::from("cue-whisper"),
            std::path::PathBuf::from("native/macos/cue-whisper/.build/cue-whisper"),
            std::path::PathBuf::from("/usr/local/bin/cue-whisper"),
        ]);
        for c in candidates {
            if c.exists() {
                return Ok(c.display().to_string());
            }
        }
        Err(WhisperError::BinaryNotFound(
            "cue-whisper not found (macOS)".into(),
        ))
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            let mut dirs = Vec::new();
            if let Some(dir) = exe.parent() {
                dirs.push(dir.to_path_buf());
            }
            if let Ok(canonical) = exe.canonicalize() {
                if let Some(dir) = canonical.parent() {
                    dirs.push(dir.to_path_buf());
                }
            }
            for dir in dirs {
                candidates.extend([dir.join("cue-whisper.exe"), dir.join("bin/cue-whisper.exe")]);
            }
        }
        candidates.extend([
            std::path::PathBuf::from("cue-whisper.exe"),
            std::path::PathBuf::from("C:\\Program Files\\Bluey\\cue-whisper.exe"),
        ]);
        for c in candidates {
            if c.exists() {
                return Ok(c.display().to_string());
            }
        }
        Err(WhisperError::BinaryNotFound(
            "cue-whisper.exe not found (Windows)".into(),
        ))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(WhisperError::BinaryNotFound("unsupported platform".into()))
    }
}

/// Spawn the helper and bridge stdin/stdout.
async fn run_helper_loop(
    binary: String,
    source: cue_core::pcm::AudioSource,
    event_tx: LatestSender<Result<TranscriptEvent, SttError>>,
    mut stdin_rx: LatestReceiver<Vec<u8>>,
    dropped_partial_events: Arc<AtomicU64>,
) {
    let mut child = match spawn_helper(&binary) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to spawn whisper helper: {e}");
            let _ = event_tx.try_send(Err(SttError::Provider(e.to_string())));
            return;
        }
    };

    let mut child_stdin = child.stdin.take();
    let child_stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = event_tx.try_send(Err(SttError::Provider("no stdout from helper".into())));
            return;
        }
    };

    // Stdout reader task
    let event_tx2 = event_tx.clone();
    let stdout_handle = tokio::spawn(async move {
        let reader = BufReader::new(child_stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            match parse_line(&line) {
                Ok(evt) => {
                    let te = evt.into_transcript_event(source);
                    if send_transcript_event(&event_tx2, te, Some(&dropped_partial_events))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    debug!(line = %line, "Ignoring unparseable whisper output: {e}");
                }
            }
        }
    });

    // Stdin writer: forward audio chunks to child stdin
    while let Some(data) = stdin_rx.recv().await {
        if let Some(ref mut pipe) = child_stdin {
            if pipe.write_all(&data).await.is_err() {
                warn!("Whisper helper stdin pipe broken");
                break;
            }
        }
    }

    // Close stdin to signal EOF
    drop(child_stdin);
    let _ = stdout_handle.await;
    let _ = child.wait().await;
}

async fn send_transcript_event(
    event_tx: &LatestSender<Result<TranscriptEvent, SttError>>,
    event: TranscriptEvent,
    dropped_partial_events: Option<&AtomicU64>,
) -> Result<(), ()> {
    let incoming_partial = matches!(event, TranscriptEvent::Partial { .. });
    let result = event_tx
        .try_send_prioritized(Ok(event), |queued| {
            queued
                .iter()
                .position(|queued| matches!(queued, Ok(TranscriptEvent::Partial { .. })))
                .or_else(|| (!incoming_partial).then_some(0))
        })
        .map_err(|_| ())?;
    let dropped_partial = match result {
        LatestSendResult::Replaced(Ok(TranscriptEvent::Partial { .. }))
        | LatestSendResult::Rejected(Ok(TranscriptEvent::Partial { .. })) => true,
        LatestSendResult::Enqueued
        | LatestSendResult::Replaced(_)
        | LatestSendResult::Rejected(_) => false,
    };
    if dropped_partial {
        if let Some(counter) = dropped_partial_events {
            count_drop(counter, "partial events");
        }
    }
    Ok(())
}

fn count_drop(counter: &AtomicU64, queue: &'static str) {
    let dropped = counter.fetch_add(1, Ordering::Relaxed) + 1;
    if dropped.is_power_of_two() {
        warn!(
            provider = "local_whisper",
            queue, dropped, "bounded realtime queue overloaded"
        );
    }
}

fn spawn_helper(binary: &str) -> Result<Child, WhisperError> {
    Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| WhisperError::SpawnFailed(format!("{binary}: {e}")))
}

#[async_trait]
impl SttProvider for LocalWhisperProvider {
    fn name(&self) -> &'static str {
        "local_whisper"
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        let tx = self.stdin_tx.as_ref().ok_or(SttError::NotActive)?;
        // Convert samples to LE bytes
        let bytes: Vec<u8> = chunk.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        match tx.try_send(bytes) {
            Ok(Some(_)) => {
                count_drop(&self.dropped_audio_chunks, "audio chunks");
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(_) => Err(SttError::Provider("helper stdin closed".into())),
        }
    }

    async fn finalize(&self) -> Result<(), SttError> {
        Ok(())
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        self.event_rx.recv().await
    }

    async fn close(&mut self) -> Result<(), SttError> {
        self.state = ConnectionState::Closed;
        self.stdin_tx = None;
        if let Some(mut handle) = self.child_handle.take() {
            if tokio::time::timeout(HELPER_SHUTDOWN_TIMEOUT, &mut handle)
                .await
                .is_err()
            {
                handle.abort();
                let _ = handle.await;
            }
        }
        Ok(())
    }
}

impl Drop for LocalWhisperProvider {
    fn drop(&mut self) {
        if let Some(handle) = &self.child_handle {
            handle.abort();
        }
    }
}

/// Returns true if local whisper is enabled via environment variable.
pub fn is_local_whisper_enabled() -> bool {
    std::env::var("BLUEY_STT_LOCAL_WHISPER")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::pcm::{AudioSource, SampleRate};

    fn chunk() -> AudioChunk {
        AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0; 320],
            captured_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn audio_overload_keeps_newest_and_close_closes_stdin_queue() {
        let (stdin_tx, mut stdin_rx) = latest_channel(1);
        let (_event_tx, event_rx) = latest_channel(1);
        let mut provider = LocalWhisperProvider {
            state: ConnectionState::Connected,
            event_rx,
            stdin_tx: Some(stdin_tx),
            child_handle: None,
            dropped_audio_chunks: AtomicU64::new(0),
            dropped_partial_events: Arc::new(AtomicU64::new(0)),
        };

        let first = chunk();
        let second = AudioChunk {
            samples: vec![2; 320],
            ..first.clone()
        };
        provider.send_audio(&first).await.unwrap();
        provider.send_audio(&second).await.unwrap();
        assert_eq!(provider.dropped_audio_chunks(), 1);
        provider.close().await.unwrap();
        assert_eq!(provider.connection_state(), ConnectionState::Closed);
        let newest = stdin_rx.recv().await.unwrap();
        assert_eq!(i16::from_le_bytes([newest[0], newest[1]]), 2);
        assert!(stdin_rx.recv().await.is_none());
        assert!(matches!(
            provider.send_audio(&chunk()).await,
            Err(SttError::NotActive)
        ));
    }

    #[tokio::test]
    async fn partial_overload_yields_to_final_without_waiting() {
        let (tx, mut rx) = latest_channel(1);
        let dropped = AtomicU64::new(0);
        let partial = TranscriptEvent::Partial {
            text: "interim".into(),
            confidence: None,
            source: AudioSource::Microphone,
        };
        send_transcript_event(&tx, partial.clone(), Some(&dropped))
            .await
            .unwrap();
        send_transcript_event(&tx, partial, Some(&dropped))
            .await
            .unwrap();
        assert_eq!(dropped.load(Ordering::Relaxed), 1);

        let final_event = TranscriptEvent::Final {
            text: "final".into(),
            confidence: None,
            source: AudioSource::Microphone,
            words: Vec::new(),
        };
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            send_transcript_event(&tx, final_event, Some(&dropped)),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(dropped.load(Ordering::Relaxed), 2);
        assert!(matches!(
            rx.recv().await,
            Some(Ok(TranscriptEvent::Final { .. }))
        ));
    }

    #[tokio::test]
    async fn close_aborts_stalled_helper_task_by_deadline() {
        let (stdin_tx, _stdin_rx) = latest_channel(1);
        let (_event_tx, event_rx) = latest_channel(1);
        let mut provider = LocalWhisperProvider {
            state: ConnectionState::Connected,
            event_rx,
            stdin_tx: Some(stdin_tx),
            child_handle: Some(tokio::spawn(std::future::pending())),
            dropped_audio_chunks: AtomicU64::new(0),
            dropped_partial_events: Arc::new(AtomicU64::new(0)),
        };

        tokio::time::timeout(std::time::Duration::from_secs(1), provider.close())
            .await
            .expect("close exceeded its helper shutdown deadline")
            .unwrap();
    }
}
