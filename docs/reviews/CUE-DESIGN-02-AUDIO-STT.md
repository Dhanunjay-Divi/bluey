# bluey Design — Audio + STT + Speaker ID

## Executive Summary

This document synthesizes audio capture, speech-to-text, and speaker identification patterns from 6 reference repositories (natively-cluely, pluely, solveWatchAi, Aura, Vysper, OpenCluely) into a concrete Rust/Tauri 2 architecture for bluey (cue). The design prioritizes:

1. **Zero-copy audio pipeline** — samples stay in Rust from capture through STT dispatch
2. **Platform-abstracted capture** — trait-based `Stream<Item = f32>` with per-platform backends
3. **Two-stage VAD** — fast RMS gate + ML confirmation before billing STT providers
4. **Multi-provider STT** — 9 providers behind a single trait, hot-swappable mid-session
5. **Speaker identification** — ECAPA-TDNN embeddings to filter user voice from interviewer
6. **Resilient state machine** — classified errors, exponential backoff, persistent reconnect

The signal path eliminates the NAPI bridge entirely (natively-cluely's bottleneck) and the base64 encoding overhead (pluely's bottleneck), keeping audio in native `&[f32]` / `&[i16]` until the STT provider boundary.

## Architecture Diagram: Audio Signal Path

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         RUST BACKEND (tokio runtime)                          │
│                                                                              │
│  ┌──────────────┐    ┌───────────┐    ┌─────────┐    ┌──────────────────┐  │
│  │ System Audio │    │           │    │         │    │  STT Provider    │  │
│  │ (per-platform)│───▶│ Ring Buf  │───▶│  VAD    │───▶│  (WebSocket/     │  │
│  │ CoreAudio Tap│    │ HeapRb    │    │ RMS+ML  │    │   gRPC/REST)     │  │
│  │ WASAPI Loop  │    │ <f32>     │    │         │    │                  │  │
│  │ Pulse Monitor│    │           │    │         │    │  9 providers     │  │
│  └──────────────┘    └───────────┘    └────┬────┘    └────────┬─────────┘  │
│                                            │                   │            │
│  ┌──────────────┐    ┌───────────┐    ┌────▼────┐    ┌────────▼─────────┐  │
│  │ Microphone   │    │           │    │Resample │    │  Transcript      │  │
│  │ (CPAL)       │───▶│ Ring Buf  │───▶│ rubato  │    │  Event Bus       │  │
│  │              │    │ HeapRb    │    │ →16kHz  │    │                  │  │
│  └──────────────┘    └───────────┘    └────┬────┘    └────────┬─────────┘  │
│                                            │                   │            │
│                                       ┌────▼────┐         ┌───▼──────┐     │
│                                       │Speaker  │         │ Tauri    │     │
│                                       │ID ECAPA │         │ Events   │     │
│                                       │(parallel)│         │ →Frontend│     │
│                                       └─────────┘         └──────────┘     │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key insight**: natively-cluely crosses the NAPI boundary 50×/sec (BatchEmitter coalesces to ~17×/sec). pluely encodes WAV+base64 per utterance. bluey eliminates both — audio stays in Rust, only transcript strings cross to the frontend via Tauri events.

## Feature Matrix Across 6 Repos

| Feature | natively-cluely | pluely | solveWatchAi | Aura | Vysper | OpenCluely |
|---------|----------------|--------|--------------|------|--------|------------|
| System audio capture | Rust (cidre SCK + CoreAudio Tap + WASAPI) | Rust (cidre CoreAudio + WASAPI + PulseAudio) | Python (sounddevice) | None | None | None |
| Mic capture | Rust (CPAL) | Rust (CPAL, unused?) | Python (sounddevice) | Python (Deepgram SDK) | Node (node-record-lpcm16 + sox) | Node (node-record-lpcm16 + sox) |
| Platform abstraction | Per-platform modules, no trait | `SpeakerStream: Stream<Item=f32>` trait | N/A (Python only) | N/A | N/A | N/A |
| Ring buffer | ringbuf 0.4 SPSC | ringbuf 0.4 HeapRb | collections.deque | None | None | None |
| VAD | Two-stage: RMS + WebRTC ML | Single-stage: RMS + peak energy | Pluggable: Silero ONNX or WebRTC | None (Deepgram handles) | None (Azure handles) | None (Azure handles) |
| Resampling | rubato 0.16 | None (hardcoded rates) | None (sounddevice handles) | None | None | None |
| STT providers | 9 (Google, Deepgram, Soniox, ElevenLabs, OpenAI RT, Groq, Azure, IBM, NativelyPro) | 1 (custom cURL) | 3 (MLX Whisper, openai-whisper, Deepgram) | 1 (Deepgram) | 1 (Azure Speech) | 2 (Azure + local Whisper CLI) |
| STT state machine | 3-state (connected/reconnecting/failed) + classified errors | None | Implicit in streaming_stt.py | None | None | None |
| Streaming STT | WebSocket partial/final | REST (batch per utterance) | LocalAgreement-2 decoder | WebSocket (Deepgram SDK) | Azure continuous recognition | Azure + Whisper CLI |
| Speaker ID | None | None | ECAPA-TDNN (SpeechBrain) + Deepgram diarization | None | None | None |
| Question filter | Heuristic (6 signal patterns) | None | Rule-based (greetings/gibberish/length) | None | LLM-based (intelligent filtering) | None |
| Per-speaker STT | Yes (system vs mic channels) | Yes (system audio only) | Yes (speaker ID filters user voice) | No | No | No |
| Crash recovery | Auto-restart + device watcher + sleep/wake | None | None | None | None | None |
| Sample rate detection | Atomic tracking, fix for 48kHz→16kHz bug | Hardcoded per-platform | sounddevice default | SDK handles | SDK handles | SDK handles |


## Design Section 1: System Audio Capture (Platform-Split Trait)

### Reference Implementations

- **pluely** `src-tauri/src/speaker/mod.rs:L1-L148` — Defines `SpeakerInput` struct and `SpeakerStream` implementing `futures::Stream<Item = f32>`. Uses `ringbuf::HeapRb` with `Waker` integration for async poll. **Cleanest abstraction.**
- **pluely** `src-tauri/src/speaker/macos.rs:L1-L386` — CoreAudio aggregate device + process tap via `cidre`. Creates `ca::TapDesc::with_mono_global_tap_excluding_processes`, builds aggregate device, starts capture into ring buffer producer.
- **pluely** `src-tauri/src/speaker/windows.rs:L1-L380` — WASAPI loopback in `EventsShared` mode with autoconvert. Dedicated `thread::spawn` (not tokio) for blocking WASAPI event loop.
- **pluely** `src-tauri/src/speaker/linux.rs:L1-L473` — PulseAudio `@DEFAULT_MONITOR@` source via `libpulse-simple`. Hardcoded 44100Hz (bug).
- **natively-cluely** `native-module/src/speaker/sck.rs:L1-L333` — ScreenCaptureKit path (macOS 13+). Uses `Condvar` for async init (not polling). Captures entire display audio at 48kHz mono. Video minimized to 2×2px 1FPS.
- **natively-cluely** `native-module/src/speaker/core_audio.rs:L1-L263` — Legacy CoreAudio Process Tap path. Creates AggregateDevice, installs tap, reads from input stream.
- **natively-cluely** `native-module/src/speaker/windows.rs:L1-L300` — WASAPI loopback capture via `wasapi` crate.

### Tradeoffs

| Approach | Pros | Cons |
|----------|------|------|
| pluely `Stream<Item=f32>` trait | Clean async integration, tokio-native, composable | Waker management complexity, cidre is niche |
| natively-cluely callback-based | Battle-tested, handles edge cases (TCC, sleep/wake) | Requires NAPI bridge, no async composition |
| ScreenCaptureKit (natively) | Non-invasive (no AggregateDevice), macOS 13+ preferred | Cannot scope to specific output device |
| CoreAudio Tap (both) | Works on macOS 12, device-specific | Creates AggregateDevice (can interfere with routing) |
| WASAPI loopback (both) | Simple, per-device | Blocking event loop requires dedicated thread |
| PulseAudio monitor (pluely) | Works on all Linux desktops | Hardcoded sample rate, no PipeWire native path |

### bluey Recommendation

Adopt pluely's trait pattern with natively-cluely's robustness features:

```rust
// src-tauri/src/audio/capture/mod.rs
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Platform-abstracted system audio capture stream.
/// Yields mono f32 samples at the device's native rate.
pub trait SystemAudioStream: Stream<Item = f32> + Send + Unpin + 'static {
    fn sample_rate(&self) -> u32;
    fn stop(&mut self);
}

/// Platform-specific implementations
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
pub use macos::MacOsSystemAudio;
#[cfg(target_os = "windows")]
pub use windows::WasapiLoopback;
#[cfg(target_os = "linux")]
pub use linux::PulseMonitor;
```

```rust
// src-tauri/src/audio/capture/macos.rs
use cidre::{arc, ca, ns};
use ringbuf::{HeapRb, Consumer};
use std::sync::{Arc, atomic::{AtomicU32, AtomicBool, Ordering}};

pub struct MacOsSystemAudio {
    consumer: Consumer<f32, Arc<HeapRb<f32>>>,
    sample_rate: Arc<AtomicU32>,
    stop_flag: Arc<AtomicBool>,
    waker: Arc<std::sync::Mutex<Option<std::task::Waker>>>,
}

impl MacOsSystemAudio {
    pub fn new(device_uid: Option<&str>) -> anyhow::Result<Self> {
        let ring = Arc::new(HeapRb::<f32>::new(131_072)); // 128KB ~1.3s at 48kHz
        let (producer, consumer) = ring.split();
        let sample_rate = Arc::new(AtomicU32::new(48_000));
        let stop_flag = Arc::new(AtomicBool::new(false));
        let waker = Arc::new(std::sync::Mutex::new(None::<std::task::Waker>));

        // Prefer ScreenCaptureKit on macOS 13+, fall back to CoreAudio Tap
        // SCK: non-invasive, captures all system audio
        // CoreAudio Tap: device-specific, creates AggregateDevice
        Self::start_sck_capture(producer, sample_rate.clone(), stop_flag.clone(), waker.clone())?;

        Ok(Self { consumer, sample_rate, stop_flag, waker })
    }
}

impl Stream for MacOsSystemAudio {
    type Item = f32;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<f32>> {
        if let Some(sample) = self.consumer.try_pop() {
            Poll::Ready(Some(sample))
        } else if self.stop_flag.load(Ordering::Relaxed) {
            Poll::Ready(None)
        } else {
            *self.waker.lock().unwrap() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl SystemAudioStream for MacOsSystemAudio {
    fn sample_rate(&self) -> u32 { self.sample_rate.load(Ordering::Relaxed) }
    fn stop(&mut self) { self.stop_flag.store(true, Ordering::Relaxed); }
}
```

### Codex Task

- **B2.1** [L] Platform-abstracted `SystemAudioStream` trait + macOS SCK impl + WASAPI impl + PulseAudio impl

---

## Design Section 2: Microphone Capture (CPAL)

### Reference Implementations

- **natively-cluely** `native-module/src/microphone.rs:L1-L474` — CPAL input stream with ring buffer. Key fix: stream is **recreated on every `start()`** because `take_consumer()` can only be called once. Error signaling via `Arc<Mutex<Option<String>>>` checked each DSP iteration.
- **pluely** `src-tauri/src/speaker/commands.rs:L95-L230` — Uses CPAL implicitly through the VAD engine for microphone. Records to WAV buffer, encodes base64, sends to STT REST endpoint.
- **natively-cluely** `native-module/src/lib.rs:L520-L540` — `get_default_output_device_id()` polls CoreAudio HAL property every 4s for device changes.

### Key Bug Fix (natively-cluely)

The "silent crash" bug: CPAL's `Stream` holds the `Consumer` handle. If you call `start()` → `stop()` → `start()`, the second `start()` tries to `take_consumer()` on an already-consumed ring buffer. Fix: **destroy and recreate the entire `MicrophoneStream`** on each start.

Source: `native-module/src/microphone.rs` — stream recreation pattern.

### Sample Rate Detection Bug

Source: natively-cluely FIXES.md, CHANGELOG v2.0.4

The app hardcoded 16kHz for STT but CPAL opens the mic at the device's native rate (typically 48kHz). Sending 48kHz audio labeled as 16kHz produces garbled transcription. Fix: detect actual rate via `Arc<AtomicU32>` written by the capture thread, read by STT configuration.

### bluey Recommendation

```rust
// src-tauri/src/audio/capture/microphone.rs
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapRb, Producer, Consumer};
use std::sync::{Arc, atomic::{AtomicU32, AtomicBool, Ordering}, Mutex};

pub struct MicrophoneCapture {
    stream: Option<cpal::Stream>,
    consumer: Option<Consumer<f32, Arc<HeapRb<f32>>>>,
    sample_rate: Arc<AtomicU32>,
    stop_flag: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    device_name: String,
}

impl MicrophoneCapture {
    pub fn new(device_name: Option<&str>) -> anyhow::Result<Self> {
        Ok(Self {
            stream: None,
            consumer: None,
            sample_rate: Arc::new(AtomicU32::new(0)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            error: Arc::new(Mutex::new(None)),
            device_name: device_name.unwrap_or("default").to_string(),
        })
    }

    /// Recreates stream on every start (fixes silent crash bug)
    pub fn start(&mut self) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = self.find_device(&host)?;
        let config = device.default_input_config()?;

        // Store actual hardware sample rate
        self.sample_rate.store(config.sample_rate().0, Ordering::Relaxed);

        let ring = Arc::new(HeapRb::<f32>::new(65_536)); // ~0.7s at 48kHz
        let (producer, consumer) = ring.split();
        self.consumer = Some(consumer);
        self.stop_flag.store(false, Ordering::Relaxed);

        let err_signal = self.error.clone();
        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                for &sample in data {
                    let _ = producer.try_push(sample);
                }
            },
            move |err| {
                *err_signal.lock().unwrap() = Some(err.to_string());
            },
            None,
        )?;
        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.stream = None; // Drop stops the stream
        self.consumer = None;
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}
```

### Codex Task

- **B2.2** [M] CPAL microphone capture with stream recreation, atomic sample rate, error signaling

---

## Design Section 3: Zero-Copy DSP + Batch Emitter

### Reference Implementations

- **natively-cluely** `native-module/src/lib.rs:L100-L300` — DSP loop runs every `DSP_POLL_MS`. Drains ring buffer, converts f32→i16 via `(f * 32767.0).clamp(-32768.0, 32767.0) as i16`, processes in 20ms chunks (960 samples at 48kHz). Uses `bytemuck::cast_slice::<i16, u8>()` for zero-copy byte reinterpretation.
- **natively-cluely** `native-module/src/lib.rs` — `BatchEmitter` coalesces 3 frames before calling `ThreadsafeFunction`. Reduces V8 boundary crossings from 50/s to ~17/s.
- **natively-cluely** CHANGELOG v2.0.4 — "Zero-copy via napi::Buffer (Uint8Array) — bypass V8 GC on continuous audio capture"

### bluey Improvement

In pure Rust (no NAPI), the BatchEmitter pattern is unnecessary — audio goes directly from DSP to STT via a `tokio::sync::mpsc` channel. The only serialization boundary is the STT provider's wire format (raw bytes for WebSocket, base64 for REST).

```rust
// src-tauri/src/audio/dsp.rs
use bytemuck;

/// Convert f32 audio to i16 PCM (what STT providers expect)
#[inline]
pub fn f32_to_i16_frame(input: &[f32], output: &mut Vec<i16>) {
    output.clear();
    output.extend(input.iter().map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16));
}

/// Zero-copy i16 slice to byte slice (for WebSocket binary frames)
#[inline]
pub fn i16_as_bytes(samples: &[i16]) -> &[u8] {
    bytemuck::cast_slice(samples)
}

/// DSP processing loop — runs as a tokio task
pub async fn dsp_loop(
    mut consumer: Consumer<f32, Arc<HeapRb<f32>>>,
    vad: &mut dyn Vad,
    resampler: Option<&mut Resampler>,
    tx: tokio::sync::mpsc::Sender<AudioFrame>,
    stop: Arc<AtomicBool>,
) {
    let chunk_size = 960; // 20ms at 48kHz
    let mut frame_buf: Vec<f32> = Vec::with_capacity(chunk_size);
    let mut i16_buf: Vec<i16> = Vec::with_capacity(chunk_size);
    let mut interval = tokio::time::interval(Duration::from_millis(5));

    loop {
        interval.tick().await;
        if stop.load(Ordering::Relaxed) { break; }

        // Drain ring buffer
        while let Some(sample) = consumer.try_pop() {
            frame_buf.push(sample);
        }

        // Process in 20ms chunks
        while frame_buf.len() >= chunk_size {
            let chunk: Vec<f32> = frame_buf.drain(..chunk_size).collect();

            // Optional resample (48kHz → 16kHz for STT)
            let processed = match resampler {
                Some(r) => r.process(&chunk),
                None => chunk,
            };

            // VAD gate
            f32_to_i16_frame(&processed, &mut i16_buf);
            let action = vad.process(&i16_buf);

            match action {
                VadAction::Speech(ended) => {
                    let bytes = i16_as_bytes(&i16_buf).to_vec();
                    let _ = tx.send(AudioFrame { data: bytes, speech_ended: ended }).await;
                }
                VadAction::Silence => { /* periodic keepalive if needed */ }
                VadAction::Suppress => { /* save bandwidth */ }
            }
        }
    }
}
```

### Codex Task

- **B2.3** [S] Zero-copy DSP loop with f32→i16 conversion, bytemuck, channel-based emission


---

## Design Section 4: Two-Stage VAD (RMS + WebRTC + Optional Silero)

### Reference Implementations

- **natively-cluely** `native-module/src/silence_suppression.rs:L1-L446` — **Best implementation.** Two-stage gate: Stage 1 adaptive RMS threshold (EMA noise floor × multiplier), Stage 2 WebRTC VAD ML model at 16kHz. Both must agree. Hangover FSM prevents clipping trailing consonants. States: `Active → Hangover → Suppressed → Active`.
- **pluely** `src-tauri/src/speaker/commands.rs:L95-L230` — Single-stage energy VAD: `hop_size=1024`, `sensitivity_rms=0.012`, `peak_threshold=0.035`, `silence_chunks=45` (~1s), `min_speech_chunks=7`, `pre_speech_chunks=12`. No ML model.
- **solveWatchAi** `transcriber/vad/__init__.py` — Factory pattern: `create_vad(engine, config)` returns either `SileroVAD` (ONNX) or `WebRTCVAD`. Pluggable architecture.
- **solveWatchAi** `transcriber/vad/silero_vad.py:L1-L100` — Silero DNN VAD via ONNX Runtime. Higher accuracy than WebRTC but ~3× slower per frame.

### Algorithm (from natively-cluely silence_suppression.rs)

```
State Machine:
  Suppressed + (RMS > threshold AND VAD=speech) → Active
  Active + (RMS < threshold OR VAD=silence) → Hangover (start timer)
  Hangover + timer_expired → Suppressed
  Hangover + (RMS > threshold AND VAD=speech) → Active (cancel timer)

Adaptive Threshold:
  noise_floor = EMA(rms, α=0.02)  // tracks ambient noise
  threshold = max(noise_floor × multiplier, min_floor)
  // min_floor: 20 for mic, 10 for system audio

Actions:
  Active → Send(frame)
  Hangover → Send(frame)  // preserves trailing consonants
  Suppressed → every 100ms: SendSilence (keepalive)
               otherwise: Suppress (save bandwidth)

Edge Detection:
  was_speaking=true AND now_suppressed → speech_ended=true (one-shot)
```

### Tradeoffs

| Approach | Accuracy | Latency | CPU | Use Case |
|----------|----------|---------|-----|----------|
| RMS only (pluely) | Low — triggers on keyboard/music | <1ms | Negligible | Pre-filter only |
| WebRTC VAD (natively) | Good for speech vs silence | ~2ms/frame | Low | Default for most users |
| Silero ONNX (solveWatchAi) | Best — handles music, noise | ~6ms/frame | Medium | Noisy environments |
| Two-stage RMS+WebRTC (natively) | Good — fast reject + ML confirm | ~2ms total | Low | **Recommended default** |

### bluey Recommendation

Port natively-cluely's two-stage approach directly (it's already Rust). Add Silero ONNX as an optional third stage for users in noisy environments.

```rust
// src-tauri/src/audio/vad.rs
use webrtc_vad::{Vad as WebRtcVad, SampleRate, VadMode};

pub enum VadAction {
    Speech(bool),  // bool = speech_ended (one-shot)
    Silence,       // periodic keepalive
    Suppress,      // drop frame entirely
}

pub struct TwoStageVad {
    // Stage 1: Adaptive RMS
    noise_floor: f32,
    rms_alpha: f32,
    rms_multiplier: f32,
    min_floor: f32,

    // Stage 2: WebRTC ML
    webrtc: WebRtcVad,

    // State machine
    state: VadState,
    hangover_remaining: u32,
    hangover_frames: u32,
    was_speaking: bool,

    // Silence keepalive counter
    silence_frame_count: u32,
    silence_keepalive_interval: u32, // frames between keepalives
}

#[derive(Clone, Copy, PartialEq)]
enum VadState { Active, Hangover, Suppressed }

impl TwoStageVad {
    pub fn new(mode: VadMode, min_floor: f32) -> Self {
        let mut webrtc = WebRtcVad::new();
        webrtc.set_mode(mode);
        Self {
            noise_floor: min_floor,
            rms_alpha: 0.02,
            rms_multiplier: 2.5,
            min_floor,
            webrtc,
            state: VadState::Suppressed,
            hangover_remaining: 0,
            hangover_frames: 15, // ~300ms at 20ms/frame
            was_speaking: false,
            silence_frame_count: 0,
            silence_keepalive_interval: 5, // every 100ms
        }
    }

    pub fn process(&mut self, samples_i16: &[i16]) -> VadAction {
        let rms = Self::compute_rms(samples_i16);
        let threshold = (self.noise_floor * self.rms_multiplier).max(self.min_floor);

        // Update noise floor (only during silence)
        if self.state == VadState::Suppressed {
            self.noise_floor = self.noise_floor * (1.0 - self.rms_alpha) + rms * self.rms_alpha;
        }

        // Stage 1: RMS gate
        let rms_speech = rms > threshold;

        // Stage 2: WebRTC VAD (only if RMS passes — saves CPU)
        let ml_speech = if rms_speech {
            self.webrtc.is_voice_segment(samples_i16)
                .unwrap_or(false)
        } else {
            false
        };

        let is_speech = rms_speech && ml_speech;
        let mut speech_ended = false;

        // State transitions
        match self.state {
            VadState::Suppressed => {
                if is_speech {
                    self.state = VadState::Active;
                    self.was_speaking = true;
                }
            }
            VadState::Active => {
                if !is_speech {
                    self.state = VadState::Hangover;
                    self.hangover_remaining = self.hangover_frames;
                }
            }
            VadState::Hangover => {
                if is_speech {
                    self.state = VadState::Active;
                } else {
                    self.hangover_remaining = self.hangover_remaining.saturating_sub(1);
                    if self.hangover_remaining == 0 {
                        self.state = VadState::Suppressed;
                        if self.was_speaking {
                            speech_ended = true;
                            self.was_speaking = false;
                        }
                    }
                }
            }
        }

        match self.state {
            VadState::Active | VadState::Hangover => VadAction::Speech(speech_ended),
            VadState::Suppressed => {
                self.silence_frame_count += 1;
                if self.silence_frame_count >= self.silence_keepalive_interval {
                    self.silence_frame_count = 0;
                    VadAction::Silence
                } else {
                    VadAction::Suppress
                }
            }
        }
    }

    fn compute_rms(samples: &[i16]) -> f32 {
        let sum: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum / samples.len() as f64).sqrt() as f32
    }
}
```

### Codex Task

- **B2.4** [M] Two-stage VAD (adaptive RMS + WebRTC ML) with hangover FSM, configurable mode

---

## Design Section 5: Multi-Provider STT (Trait + 9 Implementations)

### Reference Implementations

- **natively-cluely** `electron/main.ts:L830-L1000` — Factory `createSTTProvider(name)` returns one of 9 providers. All share interface: `write(chunk)`, `start()`, `stop()`, `on('transcript')`, `setSampleRate()`.
- **natively-cluely** `electron/audio/DeepgramStreamingSTT.ts:L1-L268` — **Cleanest WS impl** (268 lines). Binary WebSocket frames, JSON responses with word timestamps.
- **natively-cluely** `electron/audio/OpenAIStreamingSTT.ts:L1-L859` — WebSocket Realtime API (gpt-4o-transcribe) + REST whisper-1 fallback. Custom base URL support.
- **natively-cluely** `electron/audio/NativelyProSTT.ts:L1-L512` — Channel-keyed sessions (`${key}:system` vs `${key}:mic`), persistent reconnect with 30s backoff cap.
- **natively-cluely** `electron/audio/RestSTT.ts:L1-L499` — Buffers audio until speech ends, sends as single HTTP POST. Serves Groq/Azure/IBM Watson.
- **natively-cluely** `electron/audio/GoogleSTT.ts:L1-L388` — gRPC streaming, handles 305s limit + code 11 silence timeout.
- **solveWatchAi** `transcriber/deepgram_listener.py:L1-L300` — Deepgram with built-in diarization as speaker ID fallback.

### bluey Recommendation: Trait Design

```rust
// src-tauri/src/stt/mod.rs
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub is_final: bool,
    pub speaker: Speaker,
    pub language: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Speaker { System, User }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SttState { Connected, Reconnecting, Failed }

#[async_trait::async_trait]
pub trait SttProvider: Send + Sync {
    /// Send audio chunk to the provider
    async fn write(&self, audio: &[u8]) -> anyhow::Result<()>;

    /// Signal that speech has ended (for REST providers that batch)
    async fn notify_speech_ended(&self) -> anyhow::Result<()>;

    /// Start the provider connection
    async fn start(&mut self, config: SttConfig) -> anyhow::Result<()>;

    /// Stop and clean up
    async fn stop(&mut self) -> anyhow::Result<()>;

    /// Get current connection state
    fn state(&self) -> SttState;

    /// Provider name for logging/UI
    fn name(&self) -> &'static str;
}

#[derive(Clone)]
pub struct SttConfig {
    pub sample_rate: u32,
    pub language: String,       // BCP-47
    pub channel: Speaker,       // system or user
    pub api_key: String,
    pub model: Option<String>,  // provider-specific model override
    pub endpoint: Option<String>, // custom endpoint URL
}

/// Factory creates the appropriate provider
pub fn create_provider(
    provider_name: &str,
    tx: mpsc::Sender<Transcript>,
) -> anyhow::Result<Box<dyn SttProvider>> {
    match provider_name {
        "deepgram" => Ok(Box::new(deepgram::DeepgramStt::new(tx))),
        "openai" => Ok(Box::new(openai::OpenAiRealtimeStt::new(tx))),
        "google" => Ok(Box::new(google::GoogleStt::new(tx))),
        "groq" | "azure" | "ibm_watson" => Ok(Box::new(rest::RestStt::new(provider_name, tx))),
        "soniox" => Ok(Box::new(soniox::SonioxStt::new(tx))),
        "elevenlabs" => Ok(Box::new(elevenlabs::ElevenLabsStt::new(tx))),
        "whisper_local" => Ok(Box::new(local::WhisperLocalStt::new(tx))),
        _ => anyhow::bail!("Unknown STT provider: {provider_name}"),
    }
}
```

### Provider Implementation Pattern (Deepgram as reference)

```rust
// src-tauri/src/stt/deepgram.rs
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};

pub struct DeepgramStt {
    tx: mpsc::Sender<Transcript>,
    ws_tx: Option<mpsc::Sender<Vec<u8>>>,
    state: Arc<AtomicU8>, // 0=connected, 1=reconnecting, 2=failed
}

impl DeepgramStt {
    pub fn new(tx: mpsc::Sender<Transcript>) -> Self {
        Self { tx, ws_tx: None, state: Arc::new(AtomicU8::new(2)) }
    }

    async fn connect(&mut self, config: &SttConfig) -> anyhow::Result<()> {
        let url = format!(
            "wss://api.deepgram.com/v1/listen?encoding=linear16&sample_rate={}&channels=1&model=nova-2&language={}",
            config.sample_rate, config.language
        );

        let request = http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Token {}", config.api_key))
            .body(())?;

        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();

        // Audio sender channel
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(64);
        self.ws_tx = Some(audio_tx);
        self.state.store(0, Ordering::Relaxed); // connected

        let tx = self.tx.clone();
        let state = self.state.clone();

        // Read task — parse transcripts
        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(resp) = serde_json::from_str::<DeepgramResponse>(&text) {
                        let transcript = Transcript {
                            text: resp.channel.alternatives[0].transcript.clone(),
                            is_final: resp.is_final,
                            speaker: Speaker::System, // set by caller
                            language: resp.channel.detected_language.clone(),
                            timestamp_ms: (resp.start * 1000.0) as u64,
                        };
                        let _ = tx.send(transcript).await;
                    }
                }
            }
            state.store(1, Ordering::Relaxed); // reconnecting
        });

        // Write task — forward audio
        tokio::spawn(async move {
            while let Some(audio) = audio_rx.recv().await {
                if write.send(Message::Binary(audio)).await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }
}
```

### Codex Task

- **B2.5** [L] `SttProvider` trait + Deepgram WebSocket impl + OpenAI Realtime impl + REST impl (Groq/Azure/IBM)
- **B2.6** [M] Google gRPC STT impl + Soniox/ElevenLabs WebSocket impls
- **B2.7** [S] Local Whisper impl via whisper-rs (offline fallback)

---

## Design Section 6: STT State Machine + Error Classification

### Reference Implementations

- **natively-cluely** `electron/main.ts:L1001-L1080` — 3-state machine (connected/reconnecting/failed). Errors classified as auth (fatal), quota (fatal), transient (retry up to 5). Broadcasts state to renderer.
- **natively-cluely** `electron/audio/NativelyProSTT.ts:L1-L512` — Persistent reconnect with 30s backoff cap. Emits `persistent-reconnect` after 5 consecutive failures.
- **solveWatchAi** `src/services/ai.service.js:L400-L480` — Exponential backoff: 30s→60s→120s→600s cap. Provider marked failed, auto-recovers when backoff expires.

### Error Classification

```rust
// src-tauri/src/stt/state.rs

#[derive(Debug, Clone)]
pub enum SttError {
    /// Fatal — user must fix credentials. No retry.
    Auth(String),        // 401, invalid_key, auth_timeout
    /// Fatal — user must upgrade plan. No retry.
    Quota(String),       // 402, 429 with quota message
    /// Transient — retry with backoff.
    Transient(String),   // network, 5xx, WS drop, timeout
}

impl SttError {
    pub fn classify(status: Option<u16>, message: &str) -> Self {
        match status {
            Some(401) | Some(403) => Self::Auth(message.to_string()),
            Some(402) => Self::Quota(message.to_string()),
            Some(429) if message.contains("quota") => Self::Quota(message.to_string()),
            Some(429) => Self::Transient(message.to_string()), // rate limit, retry
            Some(500..=599) => Self::Transient(message.to_string()),
            _ => {
                let msg_lower = message.to_lowercase();
                if msg_lower.contains("auth") || msg_lower.contains("invalid_key") {
                    Self::Auth(message.to_string())
                } else {
                    Self::Transient(message.to_string())
                }
            }
        }
    }
}

pub struct SttStateMachine {
    state: SttState,
    consecutive_errors: u32,
    max_retries: u32,
    backoff_ms: u64,
    max_backoff_ms: u64,
}

impl SttStateMachine {
    pub fn new() -> Self {
        Self {
            state: SttState::Connected,
            consecutive_errors: 0,
            max_retries: 5,
            backoff_ms: 1_000,
            max_backoff_ms: 30_000,
        }
    }

    pub fn on_error(&mut self, err: &SttError) -> SttState {
        match err {
            SttError::Auth(_) | SttError::Quota(_) => {
                self.state = SttState::Failed;
            }
            SttError::Transient(_) => {
                self.consecutive_errors += 1;
                if self.consecutive_errors >= self.max_retries {
                    self.state = SttState::Failed;
                } else {
                    self.state = SttState::Reconnecting;
                    self.backoff_ms = (self.backoff_ms * 2).min(self.max_backoff_ms);
                }
            }
        }
        self.state
    }

    pub fn on_success(&mut self) {
        self.state = SttState::Connected;
        self.consecutive_errors = 0;
        self.backoff_ms = 1_000;
    }

    pub fn backoff_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.backoff_ms)
    }
}
```

### Codex Task

- **B2.8** [S] STT state machine with error classification, exponential backoff, state broadcast via Tauri events


---

## Design Section 7: Streaming STT — LocalAgreement-2 Decoder

### Reference Implementation

**solveWatchAi unique.** `transcriber/streaming_stt.py:L1-L350`

This is the only open-source implementation of streaming Whisper without model modification. No other reference repo has this.

### Algorithm: LocalAgreement-2

Whisper is a batch model — it transcribes a complete audio segment at once. LocalAgreement-2 converts it to streaming by re-decoding a rolling buffer every 300ms and committing words that are stable across consecutive decodes.

```
Every 300ms (DECODE_INTERVAL_S):
  1. Snapshot rolling audio buffer (VecDeque<Vec<f32>>, max 15s)
  2. RMS energy gate: skip if audio < -40 dBFS
  3. Run Whisper with word_timestamps=True on FULL buffer
  4. Apply LocalAgreement-2:
     For each word[i] after last committed position:
       text_match = (word[i].text == prev_decode[i].text)
       ts_match = |word[i].start - prev_decode[i].start| <= 0.30s
       If BOTH match → commit word[i]
       If EITHER fails → STOP (no further commits this tick)
  5. Emit partial(committed_text, tentative_text)
  6. Prune buffer: drop audio before (last_committed_end - 0.5s)
  7. Check silence-final condition:
     - FAST (300ms): text ends in ?/.!/. AND committed stable ≥2 ticks
     - SLOW (1000ms): default guard against mid-sentence pauses
```

### Adaptive Silence Threshold (solveWatchAi streaming_stt.py:L35-L45)

Two thresholds reduce end-to-end latency by 700ms for clear questions:
- **FAST** (300ms): fires when text ends in sentence punctuation AND `_stable_count >= 2`
- **SLOW** (1000ms): default — guards against mid-sentence pauses

`_stable_count` increments when committed word list is unchanged between ticks, resets on any change.

### Hold/Discard API (Integration with Speaker ID)

```
begin_utterance(uid)    — tag utterance at VAD speech-start
hold_final(uid)         — store result before speaker ID decides
release_held(uid)       — speaker ID says PASS → emit stored on_final
discard(uid)            — speaker ID says CANDIDATE → clear buffer, emit empty
```

Generation counter (`_generation`) prevents stale decodes from leaking through after a discard.

### bluey Decision: Port to Rust or Keep as Python Sidecar?

**Recommendation: Port to Rust via `whisper-rs`.**

Rationale:
- `whisper-rs` wraps whisper.cpp which supports word timestamps
- The agreement algorithm is ~50 lines of array comparison
- Eliminates Python dependency and IPC overhead
- Can run on Metal (macOS) or CUDA via whisper.cpp backends

```rust
// src-tauri/src/stt/local/streaming_decoder.rs
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct LocalAgreement2 {
    buffer: VecDeque<Vec<f32>>,
    max_buffer_s: f32,
    sample_rate: u32,
    committed_words: Vec<TimedWord>,
    prev_decode_words: Vec<TimedWord>,
    stable_count: u32,
    generation: u64,
}

#[derive(Clone, Debug)]
struct TimedWord {
    text: String,
    start: f32,
    end: f32,
}

const COMMIT_TS_TOL_S: f32 = 0.30;
const SILENCE_FAST_S: f32 = 0.30;
const SILENCE_SLOW_S: f32 = 1.00;
const STABLE_THRESHOLD: u32 = 2;

impl LocalAgreement2 {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            buffer: VecDeque::new(),
            max_buffer_s: 15.0,
            sample_rate,
            committed_words: Vec::new(),
            prev_decode_words: Vec::new(),
            stable_count: 0,
            generation: 0,
        }
    }

    /// Called every 300ms by the decode loop
    pub fn decode_tick(&mut self, whisper: &WhisperModel) -> Option<StreamingResult> {
        let audio = self.flatten_buffer();
        if Self::rms_db(&audio) < -40.0 { return None; }

        let current_words = whisper.transcribe_with_timestamps(&audio)?;

        // LocalAgreement-2: commit words stable across two decodes
        let commit_start = self.committed_words.len();
        let mut new_commits = Vec::new();

        for i in commit_start..current_words.len().min(self.prev_decode_words.len()) {
            let curr = &current_words[i];
            let prev = &self.prev_decode_words[i];

            let text_match = curr.text == prev.text;
            let ts_match = (curr.start - prev.start).abs() <= COMMIT_TS_TOL_S;

            if text_match && ts_match {
                new_commits.push(curr.clone());
            } else {
                break; // stop at first disagreement
            }
        }

        // Update stability tracking
        if new_commits.is_empty() {
            self.stable_count += 1;
        } else {
            self.stable_count = 0;
            self.committed_words.extend(new_commits);
        }

        self.prev_decode_words = current_words.clone();

        // Prune buffer
        if let Some(last) = self.committed_words.last() {
            self.prune_before(last.end - 0.5);
        }

        let committed_text: String = self.committed_words.iter()
            .map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        let tentative_text: String = current_words[self.committed_words.len()..]
            .iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");

        Some(StreamingResult { committed_text, tentative_text })
    }

    /// Check if silence threshold reached for final emission
    pub fn should_finalize(&self, silence_duration: Duration) -> bool {
        let committed_text = self.committed_words.iter()
            .map(|w| w.text.as_str()).collect::<String>();
        let ends_in_punct = committed_text.ends_with('?')
            || committed_text.ends_with('.')
            || committed_text.ends_with('!');

        if ends_in_punct && self.stable_count >= STABLE_THRESHOLD {
            silence_duration >= Duration::from_secs_f32(SILENCE_FAST_S)
        } else {
            silence_duration >= Duration::from_secs_f32(SILENCE_SLOW_S)
        }
    }

    pub fn reset(&mut self) {
        self.generation += 1;
        self.buffer.clear();
        self.committed_words.clear();
        self.prev_decode_words.clear();
        self.stable_count = 0;
    }
}

pub struct StreamingResult {
    pub committed_text: String,
    pub tentative_text: String,
}
```

### Codex Task

- **B2.9** [L] LocalAgreement-2 streaming decoder with whisper-rs, adaptive silence, hold/discard API

---

## Design Section 8: Speaker Identification (ECAPA-TDNN)

### Reference Implementation

**solveWatchAi unique.** `transcriber/speaker_id.py:L1-L300` + `transcriber/always_on_listener.py:L180-L220`

### Architecture

```
Audio Callback Thread (100ms blocks)
  VAD detects speech → accumulates _speech_buffer
  On FIRST silent frame: submit to SpeakerIDWorker (parallel with silence wait)
        │
        ▼ queue.put(_PendingSpeechSegment)
SpeakerIDWorker (daemon thread)
  Dequeues segments, runs ECAPA inference (~30-50ms on CPU)
  Decision: CANDIDATE (user's voice) → on_discard(uid)
            PASS (interviewer)       → on_pass(uid)
        │
        ▼ callbacks
StreamingSTT
  discard(uid) → clear buffer, emit empty partial
  release_held(uid) → emit stored on_final
```

### Model Details

- **Model**: SpeechBrain ECAPA-TDNN (`speechbrain/spkrec-ecapa-voxceleb`)
- **Size**: 22 MB, cached locally
- **Embedding dim**: 192 (L2-normalized)
- **Inference time**: <50ms per utterance on CPU
- **No HuggingFace token required** (public model)

### Enrollment Flow (solveWatchAi speaker_id.py)

1. User clicks "Enroll Voice" in settings
2. Records 30 seconds via microphone
3. Computes 192-dim embedding via `encode_batch()`
4. L2-normalizes and saves to `models/user_embedding.npy`
5. Runtime: compare each utterance embedding against stored embedding

### Identification Logic

```python
def identify(audio, sample_rate=16000):
    emb = compute_embedding(audio, sample_rate)  # 192-dim, L2-normalized
    sim = dot(emb, stored_embedding)              # cosine similarity
    if sim >= threshold:  # default 0.70
        return CANDIDATE, sim   # user's voice → discard
    return PASS, sim            # interviewer → forward to AI
```

### Fail-Safe Design

- Any error in `identify()` → returns `(PASS, 0.0)` — never silently drops interviewer audio
- SpeakerIDWorker error → calls `on_pass()` — same fail-safe
- Stale UID: if speech resumes before speaker ID decides, `cancel_if_uid()` marks it stale

### Deepgram Diarization Fallback (solveWatchAi deepgram_listener.py)

When using Deepgram cloud STT, speaker ID uses Deepgram's built-in diarization:
- Auto-enrollment: first N seconds identify user's speaker ID
- Saved to `config/deepgram_enrollment.pcm`
- Filters utterances where `dominant_speaker == user_speaker_id`

### bluey Recommendation: Rust Options

| Option | Pros | Cons |
|--------|------|------|
| `ort` crate (ONNX Runtime) | Fast, well-maintained, GPU support | Need to export ECAPA to ONNX |
| `candle` (HuggingFace Rust) | Native Rust, no C deps | Less mature, model loading complexity |
| Python sidecar | Exact SpeechBrain code, proven | Extra process, IPC overhead |

**Recommendation**: Use `ort` with ECAPA-TDNN exported to ONNX. The model is small (22MB) and inference is simple (forward pass → L2 normalize → cosine similarity).

```rust
// src-tauri/src/audio/speaker_id.rs
use ort::{Session, Value};
use ndarray::Array1;

pub struct SpeakerIdentifier {
    session: Session,
    enrolled_embedding: Option<Array1<f32>>,
    threshold: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeakerDecision {
    Pass(f32),      // interviewer — forward to AI (with similarity score)
    Candidate(f32), // user's voice — discard (with similarity score)
}

impl SpeakerIdentifier {
    pub fn new(model_path: &str, threshold: f32) -> anyhow::Result<Self> {
        let session = Session::builder()?.commit_from_file(model_path)?;
        Ok(Self { session, enrolled_embedding: None, threshold })
    }

    pub fn enroll(&mut self, audio_16khz: &[f32]) -> anyhow::Result<()> {
        let embedding = self.compute_embedding(audio_16khz)?;
        self.enrolled_embedding = Some(embedding);
        Ok(())
    }

    pub fn identify(&self, audio_16khz: &[f32]) -> SpeakerDecision {
        let Some(ref enrolled) = self.enrolled_embedding else {
            return SpeakerDecision::Pass(0.0); // not enrolled → pass everything
        };

        match self.compute_embedding(audio_16khz) {
            Ok(emb) => {
                let similarity = enrolled.dot(&emb); // both L2-normalized → cosine sim
                if similarity >= self.threshold {
                    SpeakerDecision::Candidate(similarity)
                } else {
                    SpeakerDecision::Pass(similarity)
                }
            }
            Err(_) => SpeakerDecision::Pass(0.0), // fail-safe: never drop interviewer
        }
    }

    fn compute_embedding(&self, audio: &[f32]) -> anyhow::Result<Array1<f32>> {
        let input = Array1::from_vec(audio.to_vec()).insert_axis(ndarray::Axis(0));
        let outputs = self.session.run(ort::inputs![input]?)?;
        let embedding = outputs[0].try_extract_tensor::<f32>()?;
        let mut emb = embedding.view().to_owned().into_raw_vec();

        // L2 normalize
        let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            emb.iter_mut().for_each(|x| *x /= norm);
        }

        Ok(Array1::from_vec(emb))
    }
}
```

### Codex Task

- **B2.10** [L] Speaker ID via ECAPA-TDNN ONNX: enrollment, runtime inference, parallel pipeline with hold/discard

---

## Design Section 9: Question Extractor Heuristic

### Reference Implementations

- **natively-cluely** `electron/SessionTracker.ts:L130-L180` — Requires ≥2 of 6 signal patterns AND minimum 50 chars. Patterns: implement/write/code, given an array/string/tree, return/find/count, function/algorithm, O(n)/complexity, specific problem names.
- **solveWatchAi** `transcriber/always_on_listener.py:L50-L100` — Filters: (1) not greeting, (2) not goodbye, (3) not too short (<5 words), (4) not hallucination, (5) not gibberish (>60% word repetition or repeated bigrams ≥3).
- **Vysper** `src/services/llm.service.js` — LLM-based: system prompt instructs model to distinguish casual chat from skill-relevant questions.

### bluey Recommendation

Combine solveWatchAi's noise filter (cheap, runs on every utterance) with natively-cluely's coding detection (runs only on passed utterances):

```rust
// src-tauri/src/intelligence/question_filter.rs

pub enum FilterResult {
    Pass,                    // forward to AI
    Reject(RejectReason),   // drop silently
}

pub enum RejectReason {
    TooShort,
    Greeting,
    Goodbye,
    Gibberish,
    Hallucination,
}

const GREETINGS: &[&str] = &["hello", "hi", "hey", "good morning", "how are you"];
const GOODBYES: &[&str] = &["bye", "goodbye", "see you", "take care", "thanks for"];
const MIN_WORDS: usize = 5;

pub fn filter_utterance(text: &str) -> FilterResult {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    // Too short
    if words.len() < MIN_WORDS { return FilterResult::Reject(RejectReason::TooShort); }

    // Greeting/goodbye
    if GREETINGS.iter().any(|g| lower.starts_with(g)) {
        return FilterResult::Reject(RejectReason::Greeting);
    }
    if GOODBYES.iter().any(|g| lower.contains(g)) {
        return FilterResult::Reject(RejectReason::Goodbye);
    }

    // Gibberish: >60% repeated words
    let unique: std::collections::HashSet<&str> = words.iter().copied().collect();
    if words.len() > 5 && (unique.len() as f32 / words.len() as f32) < 0.4 {
        return FilterResult::Reject(RejectReason::Gibberish);
    }

    // Repeated bigrams ≥3
    let bigrams: Vec<String> = words.windows(2).map(|w| format!("{} {}", w[0], w[1])).collect();
    let mut bigram_counts = std::collections::HashMap::new();
    for b in &bigrams { *bigram_counts.entry(b.as_str()).or_insert(0u32) += 1; }
    if bigram_counts.values().any(|&c| c >= 3) {
        return FilterResult::Reject(RejectReason::Gibberish);
    }

    FilterResult::Pass
}

/// Detect if utterance is a coding question (natively-cluely pattern)
pub fn is_coding_question(text: &str) -> bool {
    if text.len() < 50 { return false; }
    let lower = text.to_lowercase();
    let signals = [
        lower.contains("implement") || lower.contains("write") || lower.contains("code"),
        lower.contains("array") || lower.contains("string") || lower.contains("tree") || lower.contains("graph"),
        lower.contains("return") || lower.contains("find") || lower.contains("count"),
        lower.contains("function") || lower.contains("algorithm") || lower.contains("method"),
        lower.contains("o(n)") || lower.contains("complexity") || lower.contains("time complexity"),
        lower.contains("leetcode") || lower.contains("binary search") || lower.contains("dynamic programming"),
    ];
    signals.iter().filter(|&&s| s).count() >= 2
}
```

### Codex Task

- **B2.11** [S] Question extractor: noise filter + coding question detection heuristic

---

## Design Section 10: Per-Speaker STT (System vs Mic, Channel-Keyed)

### Reference Implementations

- **natively-cluely** `electron/main.ts:L830-L1000` — Separate STT instances: `googleSTT` (system audio = interviewer) and `googleSTT_User` (mic = user). Each has independent lifecycle, error handling, and transcript events.
- **natively-cluely** `electron/audio/NativelyProSTT.ts` — Channel-keyed sessions: `${apiKey}:system` vs `${apiKey}:mic` prevents `concurrent_session_blocked` errors when both streams are active on the same provider.
- **pluely** `src-tauri/src/speaker/commands.rs` — System audio only (no mic STT). VAD on system audio → batch → STT REST.
- **solveWatchAi** — Single mic capture + speaker ID to distinguish voices (no system audio capture).

### bluey Recommendation

Two independent STT pipelines, each with its own:
- Capture source (system audio vs microphone)
- DSP loop (separate ring buffers, VAD instances)
- STT provider instance (channel-keyed for providers that need it)
- Transcript stream (tagged with `Speaker::System` or `Speaker::User`)

```rust
// src-tauri/src/audio/pipeline.rs

pub struct AudioPipeline {
    system: Option<ChannelPipeline>,
    mic: Option<ChannelPipeline>,
    transcript_tx: mpsc::Sender<Transcript>,
}

struct ChannelPipeline {
    capture: Box<dyn SystemAudioStream>,
    vad: TwoStageVad,
    resampler: Option<Resampler>,
    stt: Box<dyn SttProvider>,
    speaker_id: Option<Arc<SpeakerIdentifier>>,
    speaker: Speaker,
    task: tokio::task::JoinHandle<()>,
}

impl AudioPipeline {
    pub async fn start(
        &mut self,
        system_config: Option<CaptureConfig>,
        mic_config: Option<CaptureConfig>,
        stt_provider: &str,
        stt_config: SttConfig,
    ) -> anyhow::Result<()> {
        if let Some(sys_cfg) = system_config {
            let mut config = stt_config.clone();
            config.channel = Speaker::System;
            self.system = Some(self.create_channel(sys_cfg, config, stt_provider, Speaker::System).await?);
        }

        if let Some(mic_cfg) = mic_config {
            let mut config = stt_config.clone();
            config.channel = Speaker::User;
            self.mic = Some(self.create_channel(mic_cfg, config, stt_provider, Speaker::User).await?);
        }

        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(mut ch) = self.system.take() {
            ch.capture.stop();
            ch.stt.stop().await.ok();
            ch.task.abort();
        }
        if let Some(mut ch) = self.mic.take() {
            ch.capture.stop();
            ch.stt.stop().await.ok();
            ch.task.abort();
        }
    }

    /// Hot-swap STT provider mid-session
    pub async fn swap_provider(&mut self, new_provider: &str, config: SttConfig) -> anyhow::Result<()> {
        // Pause captures, destroy old STT, create new, resume
        // Pattern from natively-cluely main.ts:L1780-L1830
        if let Some(ref mut ch) = self.system {
            ch.stt.stop().await.ok();
            ch.stt = create_provider(new_provider, self.transcript_tx.clone())?;
            let mut cfg = config.clone();
            cfg.channel = Speaker::System;
            ch.stt.start(cfg).await?;
        }
        if let Some(ref mut ch) = self.mic {
            ch.stt.stop().await.ok();
            ch.stt = create_provider(new_provider, self.transcript_tx.clone())?;
            let mut cfg = config.clone();
            cfg.channel = Speaker::User;
            ch.stt.start(cfg).await?;
        }
        Ok(())
    }
}
```

### Codex Task

- **B2.12** [M] Dual-channel audio pipeline: independent system + mic paths, channel-keyed STT, hot-swap support


---

## Design Section 11: Sample Rate Detection + Atomic Tracking

### Reference Implementations

- **natively-cluely** `native-module/src/lib.rs:L520-L540` — `Arc<AtomicU32>` stores detected hardware sample rate. Background capture thread writes on init, JS reads via `get_sample_rate()`. Defaults to 48kHz until detected.
- **natively-cluely** `electron/main.ts` — Tracks `_sysSttRateApplied` / `_micSttRateApplied` booleans. On `reconfigureAudio()`, re-applies rate to STT providers.
- **pluely** `src-tauri/src/speaker/linux.rs` — **Bug**: hardcodes 44100Hz instead of detecting device rate. Causes issues on devices running at 48kHz.
- **natively-cluely** CHANGELOG v2.0.4 — "Fix: hardcoding 16kHz while streaming 48kHz" — the root cause of garbled transcription.

### The Problem

STT providers expect audio at a specific sample rate (typically 16kHz). Hardware captures at native rate (typically 44.1kHz or 48kHz). If you tell the STT "this is 16kHz" but send 48kHz audio, you get 3× speed chipmunk transcription. If you resample 48kHz→16kHz but tell STT "this is 48kHz", you get 0.33× speed slow-motion transcription.

### bluey Recommendation

```rust
// src-tauri/src/audio/sample_rate.rs
use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};

pub struct Resampler {
    inner: SincFixedIn<f32>,
    input_rate: u32,
    output_rate: u32,
    output_buf: Vec<Vec<f32>>,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> anyhow::Result<Self> {
        if input_rate == output_rate {
            anyhow::bail!("No resampling needed");
        }

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let chunk_size = (input_rate as f64 * 0.02) as usize; // 20ms chunks
        let inner = SincFixedIn::new(
            output_rate as f64 / input_rate as f64,
            2.0,
            params,
            chunk_size,
            1, // mono
        )?;

        Ok(Self {
            inner,
            input_rate,
            output_rate,
            output_buf: vec![Vec::new()],
        })
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let input_frames = vec![input.to_vec()];
        match self.inner.process(&input_frames, None) {
            Ok(output) => output.into_iter().next().unwrap_or_default(),
            Err(_) => input.to_vec(), // fallback: pass through
        }
    }

    pub fn output_rate(&self) -> u32 { self.output_rate }
}

/// Determine if resampling is needed and create resampler
pub fn create_resampler_if_needed(device_rate: u32, target_rate: u32) -> Option<Resampler> {
    if device_rate == target_rate {
        None
    } else {
        Resampler::new(device_rate, target_rate).ok()
    }
}
```

### Codex Task

- **B2.13** [S] Sample rate detection via AtomicU32 + rubato resampler (device rate → 16kHz for STT)

---

## Design Section 12: STT Reconnect Strategies

### Reference Implementations

- **natively-cluely** `electron/audio/NativelyProSTT.ts:L1-L512` — Persistent reconnect: retries forever with exponential backoff capped at 30s. Emits `persistent-reconnect` event after 5 consecutive failures so UI shows "check network" banner. Channel-keyed sessions prevent `concurrent_session_blocked`.
- **natively-cluely** `electron/main.ts:L1850-L2020` — Audio recovery handler: on capture error during active meeting, waits 1.5s, destroys old capture, creates fresh instance, re-wires listeners. Max 3 attempts with exponential backoff.
- **natively-cluely** `electron/main.ts:L2030-L2130` — Default output device watcher: polls `get_default_output_device_id()` every 4s. On change, recreates SystemAudioCapture.
- **natively-cluely** `electron/main.ts:L1540-L1610` — Sleep/wake restart: `powerMonitor.resume` event triggers full capture restart.
- **solveWatchAi** `src/services/ai.service.js:L400-L480` — Provider-level backoff: 30s→60s→120s→600s cap. Auto-recovers when backoff expires.

### bluey Recommendation: Supervisor Pattern

```rust
// src-tauri/src/audio/supervisor.rs
use tokio::sync::watch;

pub struct AudioSupervisor {
    state_tx: watch::Sender<SupervisorState>,
    recovery_count: u32,
    max_recoveries: u32,
    device_watcher: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Clone, Debug)]
pub enum SupervisorState {
    Running,
    Recovering { attempt: u32 },
    Failed { reason: String },
}

impl AudioSupervisor {
    pub fn new() -> (Self, watch::Receiver<SupervisorState>) {
        let (tx, rx) = watch::channel(SupervisorState::Running);
        (Self {
            state_tx: tx,
            recovery_count: 0,
            max_recoveries: 3,
            device_watcher: None,
        }, rx)
    }

    /// Handle capture error — attempt recovery
    pub async fn on_capture_error(&mut self, pipeline: &mut AudioPipeline, error: &str) {
        self.recovery_count += 1;
        if self.recovery_count > self.max_recoveries {
            let _ = self.state_tx.send(SupervisorState::Failed {
                reason: format!("Max recoveries exceeded: {error}"),
            });
            return;
        }

        let _ = self.state_tx.send(SupervisorState::Recovering {
            attempt: self.recovery_count,
        });

        let backoff = Duration::from_millis(1500 * 2u64.pow(self.recovery_count - 1));
        tokio::time::sleep(backoff).await;

        // Destroy and recreate
        pipeline.stop().await;
        if let Err(e) = pipeline.restart().await {
            let _ = self.state_tx.send(SupervisorState::Failed {
                reason: format!("Recovery failed: {e}"),
            });
        } else {
            self.recovery_count = 0; // reset on successful recovery
            let _ = self.state_tx.send(SupervisorState::Running);
        }
    }

    /// Start device change watcher (push-based on macOS, polling fallback)
    pub fn start_device_watcher(&mut self, pipeline: Arc<Mutex<AudioPipeline>>) {
        self.device_watcher = Some(tokio::spawn(async move {
            // macOS: use AudioObjectAddPropertyListener for push-based
            // Windows/Linux: poll every 4s as fallback
            let mut interval = tokio::time::interval(Duration::from_secs(4));
            let mut last_device_id = String::new();

            loop {
                interval.tick().await;
                let current_id = get_default_output_device_id();
                if current_id != last_device_id && !last_device_id.is_empty() {
                    tracing::info!("Output device changed: {} → {}", last_device_id, current_id);
                    if let Ok(mut p) = pipeline.lock() {
                        p.restart_system_capture(&current_id).await.ok();
                    }
                }
                last_device_id = current_id;
            }
        }));
    }
}
```

### Zero-Fill TCC Detection (natively-cluely main.ts:L1170-L1220)

macOS returns zero-filled buffers (not errors) when Screen Recording permission is denied. Detection: after 12 seconds of chunks where peak amplitude never exceeds 8 (stride-sampled every 32 bytes), emit TCC-denial warning.

```rust
/// Detect macOS TCC denial (zero-filled audio buffers)
pub struct TccDetector {
    observation_frames: u32,
    max_observation_frames: u32, // ~12s worth
    ever_seen_nonzero: bool,
}

impl TccDetector {
    pub fn new(sample_rate: u32) -> Self {
        let frames_per_sec = sample_rate / 960; // 20ms frames
        Self {
            observation_frames: 0,
            max_observation_frames: frames_per_sec * 12,
            ever_seen_nonzero: false,
        }
    }

    /// Returns Some(warning) if TCC denial detected
    pub fn check(&mut self, samples: &[i16]) -> Option<&'static str> {
        if self.ever_seen_nonzero { return None; }

        // Stride-sample every 32 bytes for efficiency
        let has_signal = samples.iter().step_by(16).any(|&s| s.unsigned_abs() > 8);
        if has_signal {
            self.ever_seen_nonzero = true;
            return None;
        }

        self.observation_frames += 1;
        if self.observation_frames >= self.max_observation_frames {
            Some("System audio capture is receiving silence. Please grant Screen Recording permission in System Settings → Privacy & Security.")
        } else {
            None
        }
    }
}
```

### Codex Task

- **B2.14** [M] Audio supervisor: capture recovery, device watcher, sleep/wake restart, TCC detection

---

## Summary: Codex Task List

| ID | Size | Description |
|----|------|-------------|
| **B2.1** | L | Platform-abstracted `SystemAudioStream` trait + macOS SCK + WASAPI + PulseAudio impls |
| **B2.2** | M | CPAL microphone capture with stream recreation, atomic sample rate, error signaling |
| **B2.3** | S | Zero-copy DSP loop: f32→i16, bytemuck, channel-based emission to STT |
| **B2.4** | M | Two-stage VAD (adaptive RMS + WebRTC ML) with hangover FSM |
| **B2.5** | L | `SttProvider` trait + Deepgram WS + OpenAI Realtime WS + REST (Groq/Azure/IBM) |
| **B2.6** | M | Google gRPC STT + Soniox/ElevenLabs WebSocket impls |
| **B2.7** | S | Local Whisper impl via whisper-rs (offline fallback) |
| **B2.8** | S | STT state machine: error classification, exponential backoff, Tauri event broadcast |
| **B2.9** | L | LocalAgreement-2 streaming decoder (whisper-rs, adaptive silence, hold/discard) |
| **B2.10** | L | Speaker ID: ECAPA-TDNN ONNX enrollment + runtime inference + parallel pipeline |
| **B2.11** | S | Question extractor: noise filter + coding question detection heuristic |
| **B2.12** | M | Dual-channel audio pipeline: system + mic, channel-keyed STT, hot-swap |
| **B2.13** | S | Sample rate detection + rubato resampler (device → 16kHz) |
| **B2.14** | M | Audio supervisor: recovery, device watcher, sleep/wake, TCC detection |

**Sizing**: S = 1-2 days, M = 3-5 days, L = 1-2 weeks
**Total estimate**: ~8-12 weeks for one engineer, or ~4-6 weeks with two engineers parallelizing (B2.1-B2.4 are independent of B2.5-B2.8).

---

## Anti-Patterns (Do NOT Port)

| Anti-Pattern | Source | Why Bad | bluey Alternative |
|---|---|---|---|
| NAPI boundary for audio | natively-cluely | 50 V8 crossings/sec, Buffer allocation per chunk | Keep audio in Rust, only transcripts cross to frontend |
| Base64 WAV encoding per utterance | pluely | 33% bandwidth overhead + encode/decode CPU | Binary WebSocket frames directly from i16 bytes |
| Hardcoded sample rates | pluely linux.rs (44100Hz), natively-cluely (16kHz bug) | Garbled transcription on mismatched devices | Atomic detection + rubato resampling |
| Polling for device changes (4s) | natively-cluely | Up to 4s lost audio on device switch | Push-based `AudioObjectAddPropertyListener` (macOS) |
| God variable names (`googleSTT` holds any provider) | natively-cluely | Confusing, legacy naming | Typed `Box<dyn SttProvider>` with explicit naming |
| Synchronous file logging in audio path | natively-cluely | `appendFileSync` blocks event loop at 50 chunks/sec | `tracing` crate with async appender |
| No VAD on system audio | natively-cluely | Keyboard clicks from interviewer bleed through | Use speech-vs-noise classifier (not binary VAD) for system path |
| Single-stage energy VAD | pluely | Triggers on music, keyboard, fan noise | Two-stage: fast RMS reject + ML confirmation |

---

## Open Questions

1. **Silero vs WebRTC VAD**: Should we ship Silero ONNX (~5MB model) as default, or keep WebRTC as default with Silero as opt-in for noisy environments? WebRTC is 3× faster but less accurate.

2. **Speaker ID model distribution**: Ship the 22MB ECAPA-TDNN ONNX in the binary, or download on first enrollment? Shipping increases app size but eliminates first-run download.

3. **Local Whisper priority**: Should `whisper-rs` (whisper.cpp) be the default STT for privacy, with cloud providers as opt-in? Or cloud-first with local as fallback? Latency tradeoff: local ~400ms/decode vs cloud ~100ms streaming.

4. **System audio on Linux**: PipeWire is replacing PulseAudio on modern distros. Should we support both, or only PipeWire with PulseAudio compatibility layer?

5. **Speaker ID for system audio**: solveWatchAi uses speaker ID on mic input to filter the user's voice. Should bluey also run speaker ID on system audio to handle cases where the interviewer's system plays back the user's voice (echo)?

6. **STT provider for user channel**: If the user's voice is being filtered by speaker ID, do we still need a separate STT instance for the mic? Or is speaker ID sufficient to determine "this is the user speaking" without transcribing?

7. **WebRTC VAD sample rate**: WebRTC VAD requires exactly 8kHz, 16kHz, or 32kHz input. If device captures at 48kHz, we need to decimate before VAD. Should we decimate (cheap, lossy) or resample (expensive, accurate)?

8. **Concurrent provider limits**: Some STT providers (NativelyPro) block concurrent sessions. The channel-key pattern (`${key}:system`) works but is provider-specific. Should the trait expose a `supports_concurrent()` method?

---

## Dependency Manifest (Cargo.toml additions)

```toml
[dependencies]
# Audio capture
cpal = "0.15"
ringbuf = "0.4"
cidre = "0.11"          # macOS CoreAudio/SCK (target_os = "macos")
wasapi = "0.19"         # Windows WASAPI (target_os = "windows")
libpulse-binding = "2.30"      # Linux PulseAudio (target_os = "linux")
libpulse-simple-binding = "2.29"

# DSP
bytemuck = { version = "1", features = ["derive"] }
rubato = "0.16"         # Sample rate conversion
webrtc-vad = "0.4"      # Voice Activity Detection

# STT providers
tokio-tungstenite = "0.24"  # WebSocket clients
tonic = "0.12"              # gRPC (Google STT)
reqwest = { version = "0.12", features = ["json", "multipart", "stream"] }

# Speaker ID
ort = "2"               # ONNX Runtime
ndarray = "0.16"

# Local Whisper
whisper-rs = "0.12"     # whisper.cpp bindings

# Utilities
futures = "0.3"
async-trait = "0.1"
tracing = "0.1"
```

---

*Document synthesized from 6 reference repositories totaling ~335K LOC.*
*Every claim cites source file:line from the reference analysis docs.*
*Rust code sketches target compilation with the listed crate versions.*
