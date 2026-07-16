//! Local Whisper STT provider — spawns a native helper binary as a child process.
//!
//! The helper reads PCM16 LE 16kHz mono from stdin and emits NDJSON transcript
//! events on stdout. This provider implements the standard SttProvider trait.
//!
//! Release builds resolve only a canonical helper inside the packaged install
//! root. Debug/test builds additionally accept `BLUEY_LOCAL_WHISPER_BINARY`.

pub mod error;
pub mod parser;

use async_trait::async_trait;
use cue_core::pcm::AudioChunk;
use cue_core::stt::agreement::{AgreementOutcome, LocalAgreementConfig, LocalAgreementTracker};
use cue_core::stt::{
    ConnectionState, StableTranscriptEvent, SttConfig, SttError, SttProvider, TranscriptEvent,
};
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};
use tokio::process::{Child, Command};
use tracing::{debug, error, warn};

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use self::error::WhisperError;
use self::parser::parse_line;
use crate::audio::capture::{latest_channel, LatestReceiver, LatestSendResult, LatestSender};

const AUDIO_QUEUE_CAPACITY: usize = 50;
const EVENT_QUEUE_CAPACITY: usize = 64;
const MAX_HELPER_EVENT_BYTES: usize = 64 * 1024;
const HELPER_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
static NEXT_AGREEMENT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[cfg(unix)]
const WHISPER_CHILD_ENV_ALLOWLIST: &[&str] = &["HOME", "TMPDIR", "LANG", "LC_ALL"];
#[cfg(windows)]
const WHISPER_CHILD_ENV_ALLOWLIST: &[&str] = &["USERPROFILE", "HOME", "TEMP", "TMP"];
#[cfg(not(any(unix, windows)))]
const WHISPER_CHILD_ENV_ALLOWLIST: &[&str] = &[];

/// Local Whisper STT provider using a child-process helper binary.
pub struct LocalWhisperProvider {
    state: ConnectionState,
    event_rx: LatestReceiver<Result<StableTranscriptEvent, SttError>>,
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
fn resolve_binary() -> Result<PathBuf, WhisperError> {
    // Executable overrides are deliberately development-only. A release
    // daemon must never execute an arbitrary path inherited from its parent.
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("BLUEY_LOCAL_WHISPER_BINARY")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return canonical_executable(&path).ok_or_else(|| {
            WhisperError::BinaryNotFound(
                "BLUEY_LOCAL_WHISPER_BINARY is not a regular executable".into(),
            )
        });
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let executable = std::env::current_exe()
            .ok()
            .and_then(|path| path.canonicalize().ok())
            .ok_or_else(|| {
                WhisperError::BinaryNotFound(
                    "could not establish the packaged executable root".into(),
                )
            })?;
        let executable_dir = executable.parent().ok_or_else(|| {
            WhisperError::BinaryNotFound("packaged executable has no parent directory".into())
        })?;
        let install_root = if executable_dir.file_name().is_some_and(|name| name == "bin") {
            executable_dir.parent().unwrap_or(executable_dir)
        } else {
            executable_dir
        };
        let roots = [install_root.to_path_buf()];

        #[cfg(target_os = "macos")]
        let names = ["cue-whisper", "bluey-whisper-macos"];
        #[cfg(target_os = "windows")]
        let names = ["cue-whisper.exe", "bluey-whisper.exe"];

        for name in names {
            for candidate in [
                executable_dir.join(name),
                install_root.join("bin").join(name),
            ] {
                if let Some(binary) = canonical_packaged_executable(&candidate, &roots) {
                    return Ok(binary);
                }
            }
        }

        Err(WhisperError::BinaryNotFound(
            "packaged local transcription helper is missing".into(),
        ))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(WhisperError::BinaryNotFound("unsupported platform".into()))
    }
}

fn canonical_packaged_executable(candidate: &Path, allowed_roots: &[PathBuf]) -> Option<PathBuf> {
    let canonical = canonical_executable(candidate)?;
    allowed_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| canonical.starts_with(root))
        .then_some(canonical)
}

fn canonical_executable(candidate: &Path) -> Option<PathBuf> {
    let canonical = candidate.canonicalize().ok()?;
    let metadata = canonical.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(canonical)
}

/// Spawn the helper and bridge stdin/stdout.
async fn run_helper_loop(
    binary: PathBuf,
    source: cue_core::pcm::AudioSource,
    event_tx: LatestSender<Result<StableTranscriptEvent, SttError>>,
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
        let mut agreement = LocalAgreementTracker::with_generation(
            LocalAgreementConfig::default(),
            next_agreement_generation(),
        );
        let mut reader = BufReader::new(child_stdout);
        let mut line_buffer = Vec::with_capacity(1024);
        loop {
            let line = match read_bounded_line(&mut reader, &mut line_buffer).await {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(BoundedLineError::TooLong) => {
                    warn!(
                        max_bytes = MAX_HELPER_EVENT_BYTES,
                        "Local transcription helper emitted an oversized event"
                    );
                    let _ = event_tx2.try_send(Err(SttError::Provider(
                        "local transcription helper emitted an oversized event".into(),
                    )));
                    break;
                }
                Err(BoundedLineError::InvalidUtf8) => {
                    warn!("Local transcription helper emitted invalid UTF-8");
                    let _ = event_tx2.try_send(Err(SttError::Provider(
                        "local transcription helper emitted invalid output".into(),
                    )));
                    break;
                }
                Err(BoundedLineError::Io(error)) => {
                    debug!(error_kind = ?error.kind(), "Local transcription output closed");
                    break;
                }
            };
            if line.is_empty() {
                continue;
            }
            match parse_line(&line) {
                Ok(evt) => {
                    let event = evt.into_transcript_event(source);
                    let event = attach_agreement(&mut agreement, event);
                    if send_transcript_event(&event_tx2, event, Some(&dropped_partial_events))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) => {
                    debug!(
                        event_bytes = line.len(),
                        error = %e,
                        "Ignoring unparseable local transcription event"
                    );
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
    event_tx: &LatestSender<Result<StableTranscriptEvent, SttError>>,
    event: StableTranscriptEvent,
    dropped_partial_events: Option<&AtomicU64>,
) -> Result<(), ()> {
    let incoming_partial = event.is_partial();
    let result = event_tx
        .try_send_prioritized(Ok(event), |queued| {
            queued
                .iter()
                .position(|queued| matches!(queued, Ok(event) if event.is_partial()))
                .or_else(|| (!incoming_partial).then_some(0))
        })
        .map_err(|_| ())?;
    let dropped_partial = match result {
        LatestSendResult::Replaced(Ok(event)) | LatestSendResult::Rejected(Ok(event))
            if event.is_partial() =>
        {
            true
        }
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

fn next_agreement_generation() -> u64 {
    let generation = NEXT_AGREEMENT_GENERATION.fetch_add(1, Ordering::Relaxed);
    generation.max(1)
}

fn attach_agreement(
    tracker: &mut LocalAgreementTracker,
    event: TranscriptEvent,
) -> StableTranscriptEvent {
    let cursor = tracker.cursor();
    let outcome = match &event {
        TranscriptEvent::Partial { text, .. } => Some(tracker.observe_partial_text(cursor, text)),
        TranscriptEvent::Final { text, .. } => Some(tracker.observe_final_text(cursor, text)),
        TranscriptEvent::SpeakerLabel { .. } => None,
    };
    match outcome.and_then(AgreementOutcome::into_update) {
        Some(update) => StableTranscriptEvent::with_agreement(event, update),
        None => StableTranscriptEvent::legacy(event),
    }
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

fn spawn_helper(binary: &Path) -> Result<Child, WhisperError> {
    let mut command = Command::new(binary);
    configure_helper_command(&mut command);
    command
        .spawn()
        .map_err(|error| WhisperError::SpawnFailed(format!("{:?}", error.kind())))
}

fn configure_helper_command(command: &mut Command) {
    command
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for name in WHISPER_CHILD_ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    if let Some(root) = windows_system_root() {
        command.env("SystemRoot", &root).env("WINDIR", root);
    }
    if let Some(model) = std::env::var_os("BLUEY_WHISPER_MODEL").filter(|value| !value.is_empty()) {
        command.env("BLUEY_WHISPER_MODEL", model);
    }
}

#[cfg(windows)]
fn windows_system_root() -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;

    let mut buffer = vec![0_u16; 260];
    loop {
        let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
        if length == 0 {
            return None;
        }
        if (length as usize) < buffer.len() {
            buffer.truncate(length as usize);
            return Some(PathBuf::from(OsString::from_wide(&buffer)));
        }
        buffer.resize(length as usize + 1, 0);
    }
}

#[derive(Debug)]
enum BoundedLineError {
    TooLong,
    InvalidUtf8,
    Io(std::io::Error),
}

async fn read_bounded_line<R>(
    reader: &mut R,
    output: &mut Vec<u8>,
) -> Result<Option<String>, BoundedLineError>
where
    R: AsyncBufRead + Unpin,
{
    output.clear();
    loop {
        let available = reader.fill_buf().await.map_err(BoundedLineError::Io)?;
        if available.is_empty() {
            if output.is_empty() {
                return Ok(None);
            }
            break;
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if output.len().saturating_add(consumed) > MAX_HELPER_EVENT_BYTES {
            reader.consume(consumed);
            drain_oversized_line(reader, newline.is_some()).await?;
            return Err(BoundedLineError::TooLong);
        }
        output.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }

    if output.last() == Some(&b'\n') {
        output.pop();
    }
    if output.last() == Some(&b'\r') {
        output.pop();
    }
    String::from_utf8(output.clone())
        .map(Some)
        .map_err(|_| BoundedLineError::InvalidUtf8)
}

async fn drain_oversized_line<R>(
    reader: &mut R,
    already_reached_newline: bool,
) -> Result<(), BoundedLineError>
where
    R: AsyncBufRead + Unpin,
{
    if already_reached_newline {
        return Ok(());
    }
    loop {
        let available = reader.fill_buf().await.map_err(BoundedLineError::Io)?;
        if available.is_empty() {
            return Ok(());
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(());
        }
    }
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
        self.event_rx
            .recv()
            .await
            .map(|result| result.map(StableTranscriptEvent::into_legacy))
    }

    async fn next_stable_event(&mut self) -> Option<Result<StableTranscriptEvent, SttError>> {
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
        send_transcript_event(
            &tx,
            StableTranscriptEvent::legacy(partial.clone()),
            Some(&dropped),
        )
        .await
        .unwrap();
        send_transcript_event(&tx, StableTranscriptEvent::legacy(partial), Some(&dropped))
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
            send_transcript_event(
                &tx,
                StableTranscriptEvent::legacy(final_event),
                Some(&dropped),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(dropped.load(Ordering::Relaxed), 2);
        assert!(matches!(
            rx.recv().await,
            Some(Ok(StableTranscriptEvent {
                event: TranscriptEvent::Final { .. },
                ..
            }))
        ));
    }

    #[test]
    fn agreement_sidecar_preserves_legacy_event_and_advances_segments() {
        let mut tracker =
            LocalAgreementTracker::with_generation(LocalAgreementConfig::default(), 41);
        let first = attach_agreement(
            &mut tracker,
            TranscriptEvent::Partial {
                text: "hello".into(),
                confidence: None,
                source: AudioSource::Microphone,
            },
        );
        let first_segment = first.agreement.as_ref().unwrap().segment_id;
        assert!(matches!(
            first.event,
            TranscriptEvent::Partial { ref text, .. } if text == "hello"
        ));

        let second = attach_agreement(
            &mut tracker,
            TranscriptEvent::Partial {
                text: "hello world".into(),
                confidence: None,
                source: AudioSource::Microphone,
            },
        );
        let agreement = second.agreement.as_ref().unwrap();
        assert_eq!(agreement.generation_id, 41);
        assert_eq!(agreement.segment_id, first_segment);
        assert_eq!(agreement.committed_text, "hello");
        assert_eq!(agreement.tentative_text, "world");

        let final_event = attach_agreement(
            &mut tracker,
            TranscriptEvent::Final {
                text: "hello world".into(),
                confidence: Some(0.9),
                source: AudioSource::Microphone,
                words: Vec::new(),
            },
        );
        let agreement = final_event.agreement.as_ref().unwrap();
        assert_eq!(agreement.segment_id, first_segment);
        assert_eq!(
            agreement.phase,
            cue_core::stt::agreement::TranscriptAgreementPhase::Final
        );
        assert_ne!(tracker.cursor().segment_id, first_segment);
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

    #[tokio::test]
    async fn helper_event_reader_rejects_oversized_lines_without_buffering_the_stream() {
        let mut input = vec![b'x'; MAX_HELPER_EVENT_BYTES + 1];
        input.extend_from_slice(b"\n{\"type\":\"partial\",\"text\":\"ok\"}\n");
        let mut reader = BufReader::new(input.as_slice());
        let mut output = Vec::new();

        assert!(matches!(
            read_bounded_line(&mut reader, &mut output).await,
            Err(BoundedLineError::TooLong)
        ));
        assert!(output.len() <= MAX_HELPER_EVENT_BYTES);
        assert_eq!(
            read_bounded_line(&mut reader, &mut output)
                .await
                .unwrap()
                .as_deref(),
            Some(r#"{"type":"partial","text":"ok"}"#)
        );
    }

    #[cfg(unix)]
    #[test]
    fn packaged_helper_resolution_rejects_symlink_escape() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let base =
            std::env::temp_dir().join(format!("bluey-whisper-resolution-{}", uuid::Uuid::new_v4()));
        let root = base.join("install");
        let outside = base.join("outside-helper");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(&outside, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o700)).unwrap();

        let inside = root.join("bin/cue-whisper");
        symlink(&outside, &inside).unwrap();
        assert!(canonical_packaged_executable(&inside, std::slice::from_ref(&root)).is_none());

        std::fs::remove_file(&inside).unwrap();
        std::fs::write(&inside, b"#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&inside, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            canonical_packaged_executable(&inside, std::slice::from_ref(&root)),
            Some(inside.canonicalize().unwrap())
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn helper_environment_allowlist_excludes_search_and_injection_variables() {
        assert!(!WHISPER_CHILD_ENV_ALLOWLIST.contains(&"PATH"));
        assert!(!WHISPER_CHILD_ENV_ALLOWLIST.contains(&"LD_PRELOAD"));
        assert!(!WHISPER_CHILD_ENV_ALLOWLIST.contains(&"DYLD_INSERT_LIBRARIES"));
        assert!(!WHISPER_CHILD_ENV_ALLOWLIST.contains(&"OPENAI_API_KEY"));

        let mut command = Command::new("unused-helper");
        configure_helper_command(&mut command);
        for (name, _) in command.as_std().get_envs() {
            let name = name.to_string_lossy();
            assert!(
                WHISPER_CHILD_ENV_ALLOWLIST
                    .iter()
                    .any(|allowed| *allowed == name)
                    || name == "BLUEY_WHISPER_MODEL"
                    || (cfg!(windows) && matches!(name.as_ref(), "SystemRoot" | "WINDIR")),
                "unexpected child environment variable: {name}"
            );
        }
    }
}
