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
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            source: AudioSource::Microphone,
            chunk_ms: 20,
        }
    }
}

/// Handle to a running capture session. Drop this to stop the stream.
pub struct MicrophoneCapture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    sample_rate: SampleRate,
}

impl MicrophoneCapture {
    /// Start capture on the system's default input device.
    ///
    /// Returns a `(handle, rx)` pair: the handle owns the capture thread;
    /// `rx` yields framed `AudioChunk`s.
    pub fn start(opts: CaptureOptions) -> Result<(Self, UnboundedReceiver<AudioChunk>)> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device available")?;

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
}
