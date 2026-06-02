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
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use std::process::Stdio;

use self::error::WhisperError;
use self::parser::parse_line;

/// Local Whisper STT provider using a child-process helper binary.
pub struct LocalWhisperProvider {
    state: ConnectionState,
    event_rx: mpsc::UnboundedReceiver<Result<TranscriptEvent, SttError>>,
    stdin_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
    _child_handle: Option<tokio::task::JoinHandle<()>>,
}

impl LocalWhisperProvider {
    /// Create and start a new LocalWhisperProvider.
    pub fn connect(config: SttConfig) -> Result<Self, WhisperError> {
        let binary = resolve_binary()?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let source = config.source;
        let handle = tokio::spawn(async move {
            run_helper_loop(binary, source, event_tx, stdin_rx).await;
        });

        Ok(Self {
            state: ConnectionState::Connected,
            event_rx,
            stdin_tx: Some(stdin_tx),
            _child_handle: Some(handle),
        })
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
    event_tx: mpsc::UnboundedSender<Result<TranscriptEvent, SttError>>,
    mut stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let mut child = match spawn_helper(&binary) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to spawn whisper helper: {e}");
            let _ = event_tx.send(Err(SttError::Provider(e.to_string())));
            return;
        }
    };

    let mut child_stdin = child.stdin.take();
    let child_stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = event_tx.send(Err(SttError::Provider("no stdout from helper".into())));
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
                    if event_tx2.send(Ok(te)).is_err() {
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

fn spawn_helper(binary: &str) -> Result<Child, WhisperError> {
    Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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
        tx.send(bytes)
            .map_err(|_| SttError::Provider("helper stdin closed".into()))
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
        Ok(())
    }
}

/// Returns true if local whisper is enabled via environment variable.
pub fn is_local_whisper_enabled() -> bool {
    std::env::var("BLUEY_STT_LOCAL_WHISPER")
        .map(|v| v == "1")
        .unwrap_or(false)
}
