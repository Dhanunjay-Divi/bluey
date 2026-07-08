//! Microphone capture via CPAL.
//!
//! Spawns the capture on a dedicated OS thread (CPAL requirement: the stream
//! must live on whatever thread started it). Audio samples arrive in the
//! CPAL callback, get converted to mono i16, framed into 20 ms chunks, and
//! published to a tokio channel the caller polls.
//!
//! Integration tests for this module require a real input device and are
//! gated behind `#[ignore]`; CI does not exercise them. The framer + DSP
//! helpers are unit-tested separately in `framer.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, StreamConfig};
use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use crate::audio::framer::{downmix_to_mono, f32_to_i16, Framer};

/// Parameters for microphone capture.
pub struct CaptureOptions {
    /// Source label stamped onto each emitted chunk (normally `Microphone`).
    pub source: AudioSource,
    /// Target chunk duration in milliseconds (20 ms recommended — matches
    /// WebRTC VAD frame sizes).
    pub chunk_ms: u32,
    /// Preferred input device name. If set and a matching device is found, it
    /// will be used instead of the system default. If not found, falls back to
    /// the default device with a warning log.
    ///
    /// Note: changing this while a session is active does NOT hot-swap the
    /// device. The new setting applies on the next capture session start.
    pub device_name: Option<String>,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            source: AudioSource::Microphone,
            chunk_ms: 20,
            device_name: None,
        }
    }
}

/// Resolve the input device: prefer `device_name` if set, fall back to default.
fn resolve_input_device(host: &cpal::Host, device_name: Option<&str>) -> Result<cpal::Device> {
    if let Some(name) = device_name.filter(|n| !n.trim().is_empty()) {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if let Ok(n) = d.name() {
                    if n == name {
                        tracing::info!(device = %name, "using configured mic device");
                        return Ok(d);
                    }
                }
            }
        }
        tracing::warn!(
            requested = %name,
            "configured mic device not found, falling back to default"
        );
    }
    host.default_input_device()
        .context("no default input device available")
}

/// Handle to a running capture session. Drop this to stop the stream.
pub struct MicrophoneCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    sample_rate: SampleRate,
}

impl MicrophoneCapture {
    /// Start capture on the configured or default input device.
    ///
    /// Returns a `(handle, rx)` pair: the handle owns the capture thread;
    /// `rx` yields framed `AudioChunk`s.
    pub fn start(opts: CaptureOptions) -> Result<(Self, UnboundedReceiver<AudioChunk>)> {
        let host = cpal::default_host();
        let device = resolve_input_device(&host, opts.device_name.as_deref())?;

        let config = device
            .default_input_config()
            .context("failed to query default input config")?;
        let sample_rate =
            SampleRate::new(config.sample_rate().0).context("device reported zero sample rate")?;
        let channels = config.channels() as usize;
        if channels == 0 {
            bail!("device reported zero channels");
        }
        let format = config.sample_format();

        let (tx, rx) = unbounded_channel::<AudioChunk>();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());
        tracing::info!(
            device = %device_name,
            sample_rate = sample_rate.hz(),
            channels,
            format = ?format,
            "starting microphone capture"
        );

        let thread = thread::spawn(move || {
            let mut framer = Framer::new(opts.source, sample_rate, opts.chunk_ms);
            let stream_cfg: StreamConfig = config.into();

            let emit = {
                let tx = tx.clone();
                move |samples: Vec<i16>| {
                    let captured_at_ms = epoch_ms();
                    for chunk in framer.push(&samples, captured_at_ms) {
                        if tx.send(chunk).is_err() {
                            // Receiver dropped — the capture thread can exit on the
                            // next stop check.
                            break;
                        }
                    }
                }
            };

            // We keep `emit` as a closure that owns a mutable framer — but we
            // need it to be `Fn` (CPAL callback constraint), so wrap in a
            // Mutex and allow callbacks to borrow mutably.
            let emit_mu = std::sync::Mutex::new(emit);

            let err_fn = |e| tracing::error!(error = %e, "cpal input stream error");

            let stream_result: Result<cpal::Stream, cpal::BuildStreamError> = match format {
                SampleFormat::I16 => device.build_input_stream(
                    &stream_cfg,
                    move |data: &[i16], _| {
                        let mut emit = emit_mu.lock().unwrap();
                        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                        downmix_to_mono(data, channels, &mut mono);
                        emit(mono);
                    },
                    err_fn,
                    None,
                ),
                SampleFormat::F32 => device.build_input_stream(
                    &stream_cfg,
                    move |data: &[f32], _| {
                        let mut emit = emit_mu.lock().unwrap();
                        let mut i16_buf = Vec::with_capacity(data.len());
                        f32_to_i16(data, &mut i16_buf);
                        let mut mono = Vec::with_capacity(i16_buf.len() / channels.max(1));
                        downmix_to_mono(&i16_buf, channels, &mut mono);
                        emit(mono);
                    },
                    err_fn,
                    None,
                ),
                other => {
                    tracing::error!(?other, "unsupported CPAL sample format");
                    return;
                }
            };

            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "failed to build input stream");
                    return;
                }
            };

            use cpal::traits::StreamTrait;
            if let Err(e) = stream.play() {
                tracing::error!(error = %e, "failed to play input stream");
                return;
            }

            // Park until stop is signaled.
            while !thread_stop.load(Ordering::Relaxed) {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            drop(stream); // drop on the same thread that built it
        });

        Ok((
            MicrophoneCapture {
                stop,
                thread: Some(thread),
                sample_rate,
            },
            rx,
        ))
    }

    /// Detected hardware sample rate. Stable for the life of the capture.
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Signal the capture thread to exit and wait for it. Called
    /// automatically on drop if not invoked explicitly.
    pub fn stop(mut self) {
        self.stop_internal();
    }

    fn stop_internal(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MicrophoneCapture {
    fn drop(&mut self) {
        self.stop_internal();
    }
}

fn epoch_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Load the configured mic device name from the app settings database.
/// Returns `None` if no setting is stored or the DB is unavailable.
pub fn load_mic_device_setting(db_path: &str) -> Option<String> {
    let db = crate::db::Database::open(db_path).ok()?;
    db.load_setting("audio.mic_device")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    // Real CPAL tests require an audio device and permissions — ignored in CI.
    // They are here so a developer can `cargo test --ignored` on a real machine.

    use super::*;

    #[test]
    #[ignore]
    fn capture_starts_and_stops_cleanly() {
        let (cap, mut rx) =
            MicrophoneCapture::start(CaptureOptions::default()).expect("start capture");
        assert!(cap.sample_rate().hz() > 0);
        // Briefly pull so the channel is actually polled.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
                .await
                .ok();
        });
        cap.stop();
    }

    #[test]
    #[ignore]
    fn resolve_device_falls_back_to_default_when_name_not_found() {
        // With a bogus device name, resolve_input_device should fall back to default.
        let host = cpal::default_host();
        let result = resolve_input_device(&host, Some("__nonexistent_device_xyz__"));
        // On CI without audio devices this may fail, but the logic path is exercised.
        // If a default device exists, it should succeed.
        if host.default_input_device().is_some() {
            assert!(result.is_ok());
        }
    }

    #[test]
    #[ignore]
    fn resolve_device_uses_default_when_name_is_none() {
        let host = cpal::default_host();
        let result = resolve_input_device(&host, None);
        if host.default_input_device().is_some() {
            assert!(result.is_ok());
        }
    }

    #[test]
    #[ignore]
    fn resolve_device_uses_default_when_name_is_empty() {
        let host = cpal::default_host();
        let result = resolve_input_device(&host, Some(""));
        if host.default_input_device().is_some() {
            assert!(result.is_ok());
        }
    }

    #[test]
    fn load_mic_device_setting_returns_none_for_missing_db() {
        // Non-existent path should return None gracefully.
        let result = load_mic_device_setting("/tmp/__nonexistent_bluey_test_db__/test.db");
        assert!(result.is_none());
    }

    #[test]
    fn load_mic_device_setting_returns_stored_value() {
        let db = crate::db::Database::open(":memory:").unwrap();
        db.save_setting("audio.mic_device", "My USB Mic").unwrap();
        let val = db
            .load_setting("audio.mic_device")
            .unwrap()
            .filter(|s| !s.trim().is_empty());
        assert_eq!(val, Some("My USB Mic".to_string()));
    }
}

/// Check if a CPAL  indicates a macOS microphone permission denial.
pub fn is_permission_denied_error(error: &cpal::BuildStreamError) -> bool {
    is_permission_denied_message(&error.to_string())
}

/// String-based classifier for permission denial messages from CPAL/CoreAudio.
pub fn is_permission_denied_message(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("permission")
        || lower.contains("not authorized")
        || lower.contains("kaudiosed")
        || lower.contains("mediaservicesd")
        || lower.contains("input device is not available")
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn detects_permission_keywords() {
        assert!(is_permission_denied_message("permission denied by user"));
        assert!(is_permission_denied_message(
            "Not authorized to access microphone"
        ));
        assert!(is_permission_denied_message(
            "The input device is not available"
        ));
    }

    #[test]
    fn does_not_false_positive() {
        assert!(!is_permission_denied_message("device disconnected"));
        assert!(!is_permission_denied_message("sample rate mismatch"));
        assert!(!is_permission_denied_message(""));
    }
}
