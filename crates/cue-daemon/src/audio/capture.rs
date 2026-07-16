//! Microphone capture via CPAL.
//!
//! Spawns the capture on a dedicated OS thread (CPAL requirement: the stream
//! must live on whatever thread started it). The realtime CPAL callback only
//! copies samples into preallocated blocks and transfers them through a
//! lock-free SPSC bridge. A dedicated worker performs conversion, downmixing,
//! framing, logging, and publication to the Tokio-facing queue.
//!
//! Integration tests for this module require a real input device and are
//! gated behind `#[ignore]`; CI does not exercise them. The framer + DSP
//! helpers are unit-tested separately in `framer.rs`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, SizedSample, StreamConfig};
use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use tokio::sync::Notify;

use crate::audio::framer::{downmix_to_mono, f32_to_i16, Framer};

/// One second of 20 ms chunks at the recommended capture cadence.
const CAPTURE_QUEUE_CAPACITY: usize = 50;
/// Raw capture blocks are intentionally shorter than the downstream queue.
/// A callback larger than this is split across multiple preallocated blocks.
const RAW_BRIDGE_BLOCK_MS: usize = 20;
/// Maximum raw audio buffered between the realtime callback and worker.
const RAW_BRIDGE_CAPACITY_MS: usize = 500;
const RAW_BRIDGE_MIN_BLOCKS: usize = 4;
const MAX_CAPTURE_CHANNELS: usize = 32;
const CAPTURE_CONTROL_POLL: Duration = Duration::from_millis(10);
const CAPTURE_WORKER_IDLE_WAIT: Duration = Duration::from_millis(2);

struct LatestQueueState<T> {
    items: VecDeque<T>,
    sender_count: usize,
    receiver_open: bool,
}

struct LatestQueue<T> {
    capacity: usize,
    state: std::sync::Mutex<LatestQueueState<T>>,
    ready: Notify,
}

/// Sender for a bounded realtime queue that preserves the newest item.
pub struct LatestSender<T> {
    shared: Arc<LatestQueue<T>>,
}

/// Single-consumer receiver for [`LatestSender`].
pub struct LatestReceiver<T> {
    shared: Arc<LatestQueue<T>>,
}

pub enum LatestSendResult<T> {
    Enqueued,
    Replaced(T),
    Rejected(T),
}

/// Build a bounded queue where a full send evicts the oldest queued item.
pub fn latest_channel<T>(capacity: usize) -> (LatestSender<T>, LatestReceiver<T>) {
    assert!(capacity > 0, "latest queue capacity must be positive");
    let shared = Arc::new(LatestQueue {
        capacity,
        state: std::sync::Mutex::new(LatestQueueState {
            items: VecDeque::with_capacity(capacity),
            sender_count: 1,
            receiver_open: true,
        }),
        ready: Notify::new(),
    });
    (
        LatestSender {
            shared: Arc::clone(&shared),
        },
        LatestReceiver { shared },
    )
}

impl<T> LatestSender<T> {
    /// Enqueue `item`, returning the oldest queued item when one was evicted.
    pub fn try_send(&self, item: T) -> Result<Option<T>, T> {
        let evicted = {
            let mut state = self.shared.state.lock().unwrap();
            if !state.receiver_open {
                return Err(item);
            }
            let evicted = if state.items.len() == self.shared.capacity {
                state.items.pop_front()
            } else {
                None
            };
            state.items.push_back(item);
            evicted
        };
        self.shared.ready.notify_one();
        Ok(evicted)
    }

    /// Enqueue with a caller-selected eviction candidate. If the queue is
    /// full and `select_eviction` returns `None`, the new item is rejected.
    pub fn try_send_prioritized<F>(
        &self,
        item: T,
        select_eviction: F,
    ) -> Result<LatestSendResult<T>, T>
    where
        F: FnOnce(&VecDeque<T>) -> Option<usize>,
    {
        let result = {
            let mut state = self.shared.state.lock().unwrap();
            if !state.receiver_open {
                return Err(item);
            }
            if state.items.len() < self.shared.capacity {
                state.items.push_back(item);
                LatestSendResult::Enqueued
            } else if let Some(index) = select_eviction(&state.items) {
                let evicted = state
                    .items
                    .remove(index)
                    .expect("selected latest queue eviction must exist");
                state.items.push_back(item);
                LatestSendResult::Replaced(evicted)
            } else {
                LatestSendResult::Rejected(item)
            }
        };
        if !matches!(result, LatestSendResult::Rejected(_)) {
            self.shared.ready.notify_one();
        }
        Ok(result)
    }
}

impl<T> Clone for LatestSender<T> {
    fn clone(&self) -> Self {
        self.shared.state.lock().unwrap().sender_count += 1;
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for LatestSender<T> {
    fn drop(&mut self) {
        let last_sender = {
            let mut state = self.shared.state.lock().unwrap();
            state.sender_count = state.sender_count.saturating_sub(1);
            state.sender_count == 0
        };
        if last_sender {
            self.shared.ready.notify_waiters();
        }
    }
}

impl<T> LatestReceiver<T> {
    pub async fn recv(&mut self) -> Option<T> {
        loop {
            let notified = self.shared.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut state = self.shared.state.lock().unwrap();
                if let Some(item) = state.items.pop_front() {
                    return Some(item);
                }
                if state.sender_count == 0 {
                    return None;
                }
            }
            notified.await;
        }
    }
}

impl<T> Drop for LatestReceiver<T> {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.receiver_open = false;
        state.items.clear();
    }
}

#[derive(Debug, Clone, Copy)]
struct RawBridgeLayout {
    block_samples: usize,
    block_count: usize,
}

impl RawBridgeLayout {
    fn for_stream(sample_rate: SampleRate, channels: usize) -> Result<Self> {
        if channels == 0 {
            bail!("capture stream reported zero channels");
        }
        if channels > MAX_CAPTURE_CHANNELS {
            bail!(
                "capture stream reported {channels} channels; maximum supported is {MAX_CAPTURE_CHANNELS}"
            );
        }
        let rate = sample_rate.hz() as usize;
        let block_frames = rate
            .checked_mul(RAW_BRIDGE_BLOCK_MS)
            .context("raw capture block frame count overflow")?
            .saturating_add(999)
            / 1000;
        let block_samples = block_frames
            .max(1)
            .checked_mul(channels)
            .context("raw capture block sample count overflow")?;
        let block_count =
            RAW_BRIDGE_CAPACITY_MS.saturating_add(RAW_BRIDGE_BLOCK_MS - 1) / RAW_BRIDGE_BLOCK_MS;
        Ok(Self {
            block_samples,
            block_count: block_count.max(RAW_BRIDGE_MIN_BLOCKS),
        })
    }
}

struct RawAudioBlock<T> {
    samples: Box<[T]>,
    valid_samples: usize,
    captured_at_ms: u64,
    sequence: u64,
}

impl<T: Copy + Default> RawAudioBlock<T> {
    fn new(sample_capacity: usize) -> Self {
        Self {
            samples: vec![T::default(); sample_capacity].into_boxed_slice(),
            valid_samples: 0,
            captured_at_ms: 0,
            sequence: 0,
        }
    }
}

trait CaptureSample: SizedSample + Copy + Default + Send + 'static {
    fn append_mono(
        interleaved: &[Self],
        channels: usize,
        conversion_scratch: &mut Vec<i16>,
        mono: &mut Vec<i16>,
    );
}

impl CaptureSample for i16 {
    fn append_mono(
        interleaved: &[Self],
        channels: usize,
        _conversion_scratch: &mut Vec<i16>,
        mono: &mut Vec<i16>,
    ) {
        downmix_to_mono(interleaved, channels, mono);
    }
}

impl CaptureSample for f32 {
    fn append_mono(
        interleaved: &[Self],
        channels: usize,
        conversion_scratch: &mut Vec<i16>,
        mono: &mut Vec<i16>,
    ) {
        f32_to_i16(interleaved, conversion_scratch);
        downmix_to_mono(conversion_scratch, channels, mono);
    }
}

struct RawCallbackBridge<T> {
    filled: HeapProd<RawAudioBlock<T>>,
    recycled: HeapCons<RawAudioBlock<T>>,
    retained: Option<RawAudioBlock<T>>,
    sample_capacity: usize,
    channels: usize,
    sample_rate_hz: u32,
    next_sequence: u64,
    overflow_blocks: Arc<AtomicU64>,
    overflow_samples: Arc<AtomicU64>,
    worker_waker: Option<Thread>,
}

struct RawWorkerBridge<T> {
    filled: HeapCons<RawAudioBlock<T>>,
    recycled: HeapProd<RawAudioBlock<T>>,
}

fn build_raw_bridge<T: Copy + Default>(
    layout: RawBridgeLayout,
    channels: usize,
    sample_rate: SampleRate,
    overflow_blocks: Arc<AtomicU64>,
    overflow_samples: Arc<AtomicU64>,
) -> (RawCallbackBridge<T>, RawWorkerBridge<T>) {
    let (filled, filled_consumer) = HeapRb::<RawAudioBlock<T>>::new(layout.block_count).split();
    let (mut recycle_producer, recycled) =
        HeapRb::<RawAudioBlock<T>>::new(layout.block_count).split();
    for _ in 0..layout.block_count {
        let block = RawAudioBlock::new(layout.block_samples);
        assert!(
            recycle_producer.try_push(block).is_ok(),
            "preallocated raw capture pool must fit its recycle ring"
        );
    }
    (
        RawCallbackBridge {
            filled,
            recycled,
            retained: None,
            sample_capacity: layout.block_samples,
            channels,
            sample_rate_hz: sample_rate.hz(),
            next_sequence: 1,
            overflow_blocks,
            overflow_samples,
            worker_waker: None,
        },
        RawWorkerBridge {
            filled: filled_consumer,
            recycled: recycle_producer,
        },
    )
}

/// Realtime callback boundary. Keep this function allocation-free and limited
/// to preallocated block transfer, timestamp/sequence stamping, atomics, and
/// worker notification. Conversion, framing, logging, and async queue work
/// belong in `run_capture_worker`.
fn enqueue_raw_callback<T: Copy>(bridge: &mut RawCallbackBridge<T>, data: &[T], at_ms: u64) {
    let usable_samples = data.len() - (data.len() % bridge.channels);
    let trailing_samples = data.len() - usable_samples;
    if trailing_samples > 0 {
        bridge.overflow_blocks.fetch_add(1, Ordering::Relaxed);
        bridge
            .overflow_samples
            .fetch_add(trailing_samples as u64, Ordering::Relaxed);
    }

    let mut offset = 0usize;
    while offset < usable_samples {
        let take = (usable_samples - offset).min(bridge.sample_capacity);
        let sequence = bridge.next_sequence;
        bridge.next_sequence = bridge.next_sequence.wrapping_add(1);
        let frame_offset = offset / bridge.channels;
        let block_offset_ms = (frame_offset as u128 * 1000 / bridge.sample_rate_hz as u128) as u64;

        let Some(mut block) = bridge.retained.take().or_else(|| bridge.recycled.try_pop()) else {
            bridge.overflow_blocks.fetch_add(1, Ordering::Relaxed);
            bridge
                .overflow_samples
                .fetch_add(take as u64, Ordering::Relaxed);
            offset += take;
            continue;
        };
        block.samples[..take].copy_from_slice(&data[offset..offset + take]);
        block.valid_samples = take;
        block.captured_at_ms = at_ms.saturating_add(block_offset_ms);
        block.sequence = sequence;

        match bridge.filled.try_push(block) {
            Ok(()) => {
                if let Some(worker) = bridge.worker_waker.as_ref() {
                    worker.unpark();
                }
            }
            Err(block) => {
                // The fixed pool invariant should make this unreachable: once
                // a block has been removed from the recycle ring, the filled
                // ring has room for it. Retain it anyway so an unexpected
                // producer-full observation never frees memory in the callback.
                bridge.retained = Some(block);
                bridge.overflow_blocks.fetch_add(1, Ordering::Relaxed);
                bridge
                    .overflow_samples
                    .fetch_add(take as u64, Ordering::Relaxed);
            }
        }
        offset += take;
    }
}

#[derive(Clone, Copy)]
struct CallbackClock {
    epoch_origin_ms: u64,
    monotonic_origin: Instant,
}

impl CallbackClock {
    fn new() -> Self {
        Self {
            epoch_origin_ms: epoch_ms(),
            monotonic_origin: Instant::now(),
        }
    }

    fn now_ms(self) -> u64 {
        self.epoch_origin_ms
            .saturating_add(self.monotonic_origin.elapsed().as_millis() as u64)
    }
}

fn recycle_raw_block<T>(bridge: &mut RawWorkerBridge<T>, mut block: RawAudioBlock<T>) {
    block.valid_samples = 0;
    if bridge.recycled.try_push(block).is_err() {
        tracing::error!("raw capture recycle ring rejected a pool block");
    }
}

fn emit_worker_chunk(
    sender: &LatestSender<AudioChunk>,
    chunk: AudioChunk,
    dropped_chunks: &AtomicU64,
    downstream_closed: &AtomicBool,
) -> bool {
    if try_emit_chunk(sender, chunk, dropped_chunks) {
        true
    } else {
        downstream_closed.store(true, Ordering::Release);
        false
    }
}

struct CaptureWorkerArgs<T> {
    bridge: RawWorkerBridge<T>,
    sender: LatestSender<AudioChunk>,
    source: AudioSource,
    sample_rate: SampleRate,
    chunk_ms: u32,
    callback_closed: Arc<AtomicBool>,
    downstream_closed: Arc<AtomicBool>,
    dropped_chunks: Arc<AtomicU64>,
    worker_sample_capacity: usize,
    channels: usize,
}

fn run_capture_worker<T: CaptureSample>(args: CaptureWorkerArgs<T>) {
    let CaptureWorkerArgs {
        mut bridge,
        sender,
        source,
        sample_rate,
        chunk_ms,
        callback_closed,
        downstream_closed,
        dropped_chunks,
        worker_sample_capacity,
        channels,
    } = args;
    let mut framer = Framer::new(source, sample_rate, chunk_ms);
    let mut conversion_scratch = Vec::with_capacity(worker_sample_capacity);
    let mut mono = Vec::with_capacity(worker_sample_capacity / channels.max(1));
    let mut last_sequence: Option<u64> = None;

    loop {
        let Some(block) = bridge.filled.try_pop() else {
            if callback_closed.load(Ordering::Acquire) {
                break;
            }
            thread::park_timeout(CAPTURE_WORKER_IDLE_WAIT);
            continue;
        };

        let sequence_gap = last_sequence
            .map(|last| block.sequence != last.wrapping_add(1))
            .unwrap_or(false);
        if sequence_gap {
            if let Some(chunk) = framer.flush_padded() {
                if !emit_worker_chunk(&sender, chunk, &dropped_chunks, &downstream_closed) {
                    recycle_raw_block(&mut bridge, block);
                    return;
                }
            }
        }
        last_sequence = Some(block.sequence);

        conversion_scratch.clear();
        mono.clear();
        T::append_mono(
            &block.samples[..block.valid_samples],
            channels,
            &mut conversion_scratch,
            &mut mono,
        );
        let chunks = framer.push(&mono, block.captured_at_ms);
        let mut keep_running = true;
        for chunk in chunks {
            if !emit_worker_chunk(&sender, chunk, &dropped_chunks, &downstream_closed) {
                keep_running = false;
                break;
            }
        }
        recycle_raw_block(&mut bridge, block);
        if !keep_running {
            return;
        }
    }

    if let Some(chunk) = framer.flush_padded() {
        let _ = emit_worker_chunk(&sender, chunk, &dropped_chunks, &downstream_closed);
    }
}

struct CaptureStreamArgs {
    device: cpal::Device,
    stream_config: StreamConfig,
    source: AudioSource,
    chunk_ms: u32,
    sample_rate: SampleRate,
    channels: usize,
    stop: Arc<AtomicBool>,
    sender: LatestSender<AudioChunk>,
    dropped_chunks: Arc<AtomicU64>,
    raw_overflow_blocks: Arc<AtomicU64>,
    raw_overflow_samples: Arc<AtomicU64>,
}

fn run_capture_stream<T: CaptureSample>(args: CaptureStreamArgs) {
    let CaptureStreamArgs {
        device,
        stream_config,
        source,
        chunk_ms,
        sample_rate,
        channels,
        stop,
        sender,
        dropped_chunks,
        raw_overflow_blocks,
        raw_overflow_samples,
    } = args;
    let layout = match RawBridgeLayout::for_stream(sample_rate, channels) {
        Ok(layout) => layout,
        Err(error) => {
            tracing::error!(error = %error, "invalid microphone capture bridge layout");
            return;
        }
    };
    let (mut callback_bridge, worker_bridge) = build_raw_bridge::<T>(
        layout,
        channels,
        sample_rate,
        Arc::clone(&raw_overflow_blocks),
        Arc::clone(&raw_overflow_samples),
    );
    let callback_closed = Arc::new(AtomicBool::new(false));
    let downstream_closed = Arc::new(AtomicBool::new(false));
    let worker_callback_closed = Arc::clone(&callback_closed);
    let worker_downstream_closed = Arc::clone(&downstream_closed);
    let worker_dropped_chunks = Arc::clone(&dropped_chunks);
    let worker = match thread::Builder::new()
        .name("bluey-mic-worker".to_string())
        .spawn(move || {
            run_capture_worker(CaptureWorkerArgs {
                bridge: worker_bridge,
                sender,
                source,
                sample_rate,
                chunk_ms,
                callback_closed: worker_callback_closed,
                downstream_closed: worker_downstream_closed,
                dropped_chunks: worker_dropped_chunks,
                worker_sample_capacity: layout.block_samples,
                channels,
            });
        }) {
        Ok(worker) => worker,
        Err(error) => {
            tracing::error!(error = %error, "failed to start microphone capture worker");
            return;
        }
    };
    callback_bridge.worker_waker = Some(worker.thread().clone());

    let callback_clock = CallbackClock::new();
    let stream_error_count = Arc::new(AtomicU64::new(0));
    let callback_error_count = Arc::clone(&stream_error_count);
    let stream_result = device.build_input_stream::<T, _, _>(
        &stream_config,
        move |data, _| {
            enqueue_raw_callback(&mut callback_bridge, data, callback_clock.now_ms());
        },
        move |_error| {
            callback_error_count.fetch_add(1, Ordering::Relaxed);
        },
        None,
    );

    let stream = match stream_result {
        Ok(stream) => stream,
        Err(error) => {
            tracing::error!(error = %error, "failed to build input stream");
            callback_closed.store(true, Ordering::Release);
            worker.thread().unpark();
            let _ = worker.join();
            return;
        }
    };

    use cpal::traits::StreamTrait;
    if let Err(error) = stream.play() {
        tracing::error!(error = %error, "failed to play input stream");
        drop(stream);
        callback_closed.store(true, Ordering::Release);
        worker.thread().unpark();
        let _ = worker.join();
        return;
    }

    let mut next_raw_overflow_report = 1u64;
    while !stop.load(Ordering::Acquire)
        && !downstream_closed.load(Ordering::Acquire)
        && !worker.is_finished()
    {
        let stream_errors = stream_error_count.swap(0, Ordering::AcqRel);
        if stream_errors > 0 {
            tracing::error!(
                count = stream_errors,
                "CPAL reported microphone input stream errors"
            );
        }
        let raw_overflow = raw_overflow_blocks.load(Ordering::Relaxed);
        if raw_overflow >= next_raw_overflow_report {
            tracing::warn!(
                dropped_blocks = raw_overflow,
                dropped_samples = raw_overflow_samples.load(Ordering::Relaxed),
                "microphone raw capture bridge overloaded"
            );
            next_raw_overflow_report = raw_overflow.checked_next_power_of_two().unwrap_or(u64::MAX);
            if next_raw_overflow_report <= raw_overflow {
                next_raw_overflow_report = next_raw_overflow_report.saturating_mul(2);
            }
        }
        thread::sleep(CAPTURE_CONTROL_POLL);
    }

    // Dropping the stream first guarantees no callback can enqueue after the
    // worker observes callback_closed and drains the final filled blocks.
    drop(stream);
    callback_closed.store(true, Ordering::Release);
    worker.thread().unpark();
    if worker.join().is_err() {
        tracing::error!("microphone capture worker panicked");
    }
}

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
    dropped_chunks: Arc<AtomicU64>,
    raw_overflow_blocks: Arc<AtomicU64>,
    raw_overflow_samples: Arc<AtomicU64>,
}

impl MicrophoneCapture {
    /// Start capture on the configured or default input device.
    ///
    /// Returns a `(handle, rx)` pair: the handle owns the capture thread;
    /// `rx` yields framed `AudioChunk`s.
    pub fn start(opts: CaptureOptions) -> Result<(Self, LatestReceiver<AudioChunk>)> {
        if opts.chunk_ms == 0 {
            bail!("capture chunk duration must be non-zero");
        }
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
        RawBridgeLayout::for_stream(sample_rate, channels)?;
        let format = config.sample_format();

        let (tx, rx) = latest_channel::<AudioChunk>(CAPTURE_QUEUE_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let dropped_chunks = Arc::new(AtomicU64::new(0));
        let raw_overflow_blocks = Arc::new(AtomicU64::new(0));
        let raw_overflow_samples = Arc::new(AtomicU64::new(0));
        let thread_stop = stop.clone();
        let thread_dropped_chunks = dropped_chunks.clone();
        let thread_raw_overflow_blocks = raw_overflow_blocks.clone();
        let thread_raw_overflow_samples = raw_overflow_samples.clone();
        let device_name = device.name().unwrap_or_else(|_| "<unknown>".into());
        tracing::info!(
            device = %device_name,
            sample_rate = sample_rate.hz(),
            channels,
            format = ?format,
            "starting microphone capture"
        );

        let thread = thread::spawn(move || {
            let stream_cfg: StreamConfig = config.into();
            let args = CaptureStreamArgs {
                device,
                stream_config: stream_cfg,
                source: opts.source,
                chunk_ms: opts.chunk_ms,
                sample_rate,
                channels,
                stop: thread_stop,
                sender: tx,
                dropped_chunks: thread_dropped_chunks,
                raw_overflow_blocks: thread_raw_overflow_blocks,
                raw_overflow_samples: thread_raw_overflow_samples,
            };
            match format {
                SampleFormat::I16 => run_capture_stream::<i16>(args),
                SampleFormat::F32 => run_capture_stream::<f32>(args),
                other => {
                    tracing::error!(?other, "unsupported CPAL sample format");
                }
            }
        });

        Ok((
            MicrophoneCapture {
                stop,
                thread: Some(thread),
                sample_rate,
                dropped_chunks,
                raw_overflow_blocks,
                raw_overflow_samples,
            },
            rx,
        ))
    }

    /// Detected hardware sample rate. Stable for the life of the capture.
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }

    /// Number of pre-framing raw blocks dropped because the realtime bridge
    /// was saturated or received a trailing partial channel frame.
    pub fn raw_overflow_blocks(&self) -> u64 {
        self.raw_overflow_blocks.load(Ordering::Relaxed)
    }

    /// Number of interleaved device samples dropped at the raw bridge.
    pub fn raw_overflow_samples(&self) -> u64 {
        self.raw_overflow_samples.load(Ordering::Relaxed)
    }

    /// Signal the capture thread to exit and wait for it. Called
    /// automatically on drop if not invoked explicitly.
    pub fn stop(mut self) {
        self.stop_internal();
    }

    fn stop_internal(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn try_emit_chunk(
    sender: &LatestSender<AudioChunk>,
    chunk: AudioChunk,
    dropped_chunks: &AtomicU64,
) -> bool {
    match sender.try_send(chunk) {
        Ok(Some(_)) => {
            let dropped = dropped_chunks.fetch_add(1, Ordering::Relaxed) + 1;
            if dropped.is_power_of_two() {
                tracing::warn!(dropped, "microphone capture queue overloaded");
            }
            true
        }
        Ok(None) => true,
        Err(_) => false,
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

    #[tokio::test]
    async fn capture_queue_drops_on_overload_and_detects_closed_receiver() {
        let (tx, mut rx) = latest_channel(1);
        let dropped = AtomicU64::new(0);
        let chunk = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0; 320],
            captured_at_ms: 0,
        };

        assert!(try_emit_chunk(&tx, chunk.clone(), &dropped));
        let newest = AudioChunk {
            captured_at_ms: 1,
            ..chunk.clone()
        };
        assert!(try_emit_chunk(&tx, newest, &dropped));
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(rx.recv().await.unwrap().captured_at_ms, 1);
        drop(rx);
        assert!(!try_emit_chunk(&tx, chunk, &dropped));
    }

    #[test]
    fn raw_bridge_saturation_is_bounded_and_preserves_accepted_order() {
        let overflow_blocks = Arc::new(AtomicU64::new(0));
        let overflow_samples = Arc::new(AtomicU64::new(0));
        let layout = RawBridgeLayout {
            block_samples: 2,
            block_count: 2,
        };
        let (mut callback, mut worker) = build_raw_bridge::<i16>(
            layout,
            1,
            SampleRate::new(1_000).unwrap(),
            Arc::clone(&overflow_blocks),
            Arc::clone(&overflow_samples),
        );

        enqueue_raw_callback(&mut callback, &[1, 2, 3, 4, 5, 6], 100);

        assert_eq!(overflow_blocks.load(Ordering::Relaxed), 1);
        assert_eq!(overflow_samples.load(Ordering::Relaxed), 2);
        let first = worker.filled.try_pop().expect("first accepted block");
        let second = worker.filled.try_pop().expect("second accepted block");
        assert_eq!(&first.samples[..first.valid_samples], &[1, 2]);
        assert_eq!(&second.samples[..second.valid_samples], &[3, 4]);
        assert_eq!((first.sequence, second.sequence), (1, 2));
        assert_eq!((first.captured_at_ms, second.captured_at_ms), (100, 102));
        assert!(worker.filled.try_pop().is_none());
    }

    #[tokio::test]
    async fn raw_worker_preserves_order_and_flushes_padded_tail() {
        let overflow_blocks = Arc::new(AtomicU64::new(0));
        let overflow_samples = Arc::new(AtomicU64::new(0));
        let layout = RawBridgeLayout {
            block_samples: 3,
            block_count: 2,
        };
        let (mut callback, worker) = build_raw_bridge::<i16>(
            layout,
            1,
            SampleRate::new(1_000).unwrap(),
            overflow_blocks,
            overflow_samples,
        );
        enqueue_raw_callback(&mut callback, &[1, 2, 3, 4, 5, 6], 200);

        let callback_closed = Arc::new(AtomicBool::new(true));
        let downstream_closed = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let (sender, mut receiver) = latest_channel(8);
        run_capture_worker(CaptureWorkerArgs {
            bridge: worker,
            sender,
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(1_000).unwrap(),
            chunk_ms: 4,
            callback_closed,
            downstream_closed: Arc::clone(&downstream_closed),
            dropped_chunks: Arc::clone(&dropped),
            worker_sample_capacity: layout.block_samples,
            channels: 1,
        });

        let first = receiver.recv().await.expect("complete frame");
        let final_padded = receiver.recv().await.expect("padded final frame");
        assert_eq!(first.samples, vec![1, 2, 3, 4]);
        assert_eq!(first.captured_at_ms, 200);
        assert_eq!(final_padded.samples, vec![5, 6, 0, 0]);
        assert_eq!(final_padded.captured_at_ms, 204);
        assert!(receiver.recv().await.is_none());
        assert!(!downstream_closed.load(Ordering::Acquire));
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn raw_worker_flushes_before_a_sequence_gap() {
        let overflow_blocks = Arc::new(AtomicU64::new(0));
        let overflow_samples = Arc::new(AtomicU64::new(0));
        let layout = RawBridgeLayout {
            block_samples: 2,
            block_count: 2,
        };
        let (mut callback, worker) = build_raw_bridge::<i16>(
            layout,
            1,
            SampleRate::new(1_000).unwrap(),
            overflow_blocks,
            overflow_samples,
        );
        enqueue_raw_callback(&mut callback, &[1, 2], 300);
        callback.next_sequence = 3;
        enqueue_raw_callback(&mut callback, &[3, 4], 304);

        let callback_closed = Arc::new(AtomicBool::new(true));
        let downstream_closed = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicU64::new(0));
        let (sender, mut receiver) = latest_channel(8);
        run_capture_worker(CaptureWorkerArgs {
            bridge: worker,
            sender,
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(1_000).unwrap(),
            chunk_ms: 4,
            callback_closed,
            downstream_closed,
            dropped_chunks: dropped,
            worker_sample_capacity: layout.block_samples,
            channels: 1,
        });

        let before_gap = receiver.recv().await.expect("pre-gap padded frame");
        let after_gap = receiver.recv().await.expect("post-gap padded frame");
        assert_eq!(before_gap.samples, vec![1, 2, 0, 0]);
        assert_eq!(before_gap.captured_at_ms, 300);
        assert_eq!(after_gap.samples, vec![3, 4, 0, 0]);
        assert_eq!(after_gap.captured_at_ms, 304);
        assert!(receiver.recv().await.is_none());
    }

    #[test]
    fn raw_worker_stops_cleanly_after_callback_close() {
        let layout = RawBridgeLayout {
            block_samples: 4,
            block_count: 2,
        };
        let (_callback, worker_bridge) = build_raw_bridge::<i16>(
            layout,
            1,
            SampleRate::new(1_000).unwrap(),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
        );
        let callback_closed = Arc::new(AtomicBool::new(false));
        let worker_closed = Arc::clone(&callback_closed);
        let (sender, _receiver) = latest_channel(2);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            run_capture_worker(CaptureWorkerArgs {
                bridge: worker_bridge,
                sender,
                source: AudioSource::Microphone,
                sample_rate: SampleRate::new(1_000).unwrap(),
                chunk_ms: 4,
                callback_closed: worker_closed,
                downstream_closed: Arc::new(AtomicBool::new(false)),
                dropped_chunks: Arc::new(AtomicU64::new(0)),
                worker_sample_capacity: layout.block_samples,
                channels: 1,
            });
            let _ = done_tx.send(());
        });

        callback_closed.store(true, Ordering::Release);
        worker.thread().unpark();
        done_rx
            .recv_timeout(Duration::from_millis(250))
            .expect("worker did not stop after callback close");
        worker.join().expect("worker join");
    }

    #[test]
    fn realtime_callback_boundary_contains_no_forbidden_work() {
        let source = include_str!("capture.rs");
        let callback = source
            .split("fn enqueue_raw_callback")
            .nth(1)
            .expect("callback function exists")
            .split("#[derive(Clone, Copy)]")
            .next()
            .expect("callback section ends before clock");
        for forbidden in [
            "Mutex",
            "Vec::",
            "Box",
            "vec!",
            "String",
            "format!",
            "downmix",
            "f32_to_i16",
            "tracing::",
            "tokio",
            "Framer",
            "AudioChunk",
            "try_emit_chunk",
        ] {
            assert!(
                !callback.contains(forbidden),
                "realtime callback contains forbidden work: {forbidden}"
            );
        }
    }

    #[tokio::test]
    async fn latest_queue_evicts_oldest_and_closes_after_last_sender() {
        let (tx, mut rx) = latest_channel(2);
        assert_eq!(tx.try_send(1).unwrap(), None);
        assert_eq!(tx.try_send(2).unwrap(), None);
        assert_eq!(tx.try_send(3).unwrap(), Some(1));

        assert_eq!(rx.recv().await, Some(2));
        assert_eq!(rx.recv().await, Some(3));
        drop(tx);
        assert_eq!(rx.recv().await, None);
    }

    #[tokio::test]
    async fn latest_queue_does_not_lose_sender_receiver_races() {
        for value in 0..100 {
            let (tx, mut rx) = latest_channel(1);
            let sender = tokio::spawn(async move {
                tokio::task::yield_now().await;
                tx.try_send(value).unwrap();
            });
            assert_eq!(
                tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
                    .await
                    .expect("latest queue lost a wakeup"),
                Some(value)
            );
            sender.await.unwrap();
        }
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
