# cue-transcribe — Fresh Build Plan (real-time, cross-platform, on-device STT)

> The single source of truth for the transcription engine rebuild. Goals locked
> with the user 2026-06-29. We copy the proven `nifty-brahmagupta` prototype,
> make it cross-platform + production-grade, and wire it into Bluey. Research is
> grounded (see Sources at bottom) so we build each hard part right the first time.

## 1. Goal (one sentence)

A lean, scalable, production-grade engine that transcribes **system audio (them) +
mic (you)** in **real time, on-device, accurately**, on **macOS (Apple Silicon +
Intel) and Windows**, and saves a clean transcript.

## 2. What we are NOT doing (the traps that wasted a day)
- ❌ NOT capturing audio inside the Tauri overlay webview (`getUserMedia`) — the
  browser can't capture native-app/system audio on macOS, and it dragged in
  .app-bundle/permission/respawn hell.
- ❌ NOT relying on CoreML — parakeet-rs's own author says CoreML is unstable for
  this model and **CPU is faster than Whisper-metal on M-series anyway**.
- ❌ NOT a bare CLI helper for macOS system audio — a plain executable **cannot
  appear in System Settings → Screen Recording**, and ad-hoc signing **wipes the
  grant on every rebuild**. (This was the root of today's silent-audio nightmare.)
- ❌ NOT rebuilding the UI — the existing overlay UI stays.

## 3. Architecture (grounded, cross-platform)

```
 capture (per-OS)  ──16kHz mono PCM16──►  STT engine (shared)  ──►  transcript out
 ────────────────                         ─────────────────         ─────────────
 macOS system : ScreenCaptureKit          parakeet-rs Nemotron      labeled lines
   (signed .app, not a bare CLI)            streaming, 560ms chunks  (You / They),
 macOS mic    : cpal                        ort CPU EP (default)     saved + live
 Windows sys  : wasapi crate (loopback)     same model, same code    feed to Bluey
 Windows mic  : cpal/wasapi
```

### 3a. Capture layer (the hard, per-OS part)
| Source | macOS | Windows |
|--------|-------|---------|
| **System (them)** | ScreenCaptureKit, `capturesAudio=true`. **MUST ship as a codesigned `.app` bundle** with a stable signing identity + deployment target ≥ 14.4, and call `CGRequestScreenCaptureAccess()` at startup so it (a) appears in Screen Recording settings and (b) the grant survives rebuilds. | `wasapi` crate loopback: `get_default_device(Direction::Render)` → loopback capture client, `Direction::Capture`, `ShareMode::Shared`. **Poll for data — event mode does NOT work for loopback.** |
| **Mic (you)** | `cpal` default input | `cpal` (or `wasapi` capture) |

All capture normalizes to **16kHz mono PCM16** (the model's input), framed in
~100ms chunks (the prototype's proven `pcm-worklet` ratio, done natively here).

### 3b. STT engine (shared across platforms — copied from prototype)
- `parakeet-rs` Nemotron **streaming** (cache-aware, 560ms chunks, punctuation).
- `ort` with the **CPU execution provider as the default everywhere** (stable +
  fast). DirectML (Windows) / CoreML (macOS) are optional, behind cargo features,
  only if measured to help — default is CPU.
- ONE provider per source (mic + system are separate stateful streams — proven
  earlier: sharing one stateful model across speakers corrupts the transcript).

### 3c. Cargo feature/target matrix (from the prototype, extended for Windows)
```toml
[target.'cfg(target_os = "macos")'.dependencies]
parakeet-rs = { version = "0.3", default-features = false, features = ["sortformer"] } # CPU default
# (CoreML feature available but OFF by default — unstable for this model)

[target.'cfg(target_os = "windows")'.dependencies]
parakeet-rs = { version = "0.3", default-features = false, features = ["sortformer"] } # CPU
wasapi = "0.x"  # system-audio loopback
```

## 4. Crate layout: `crates/cue-transcribe` (new, self-contained)
```
cue-transcribe/
  Cargo.toml
  src/
    lib.rs           # public API: start(sources) -> stream of TranscriptLine
    engine.rs        # parakeet Nemotron streaming (copied from prototype nemo_session.rs)
    capture/
      mod.rs         # trait AudioCapture -> 16k mono PCM16 frames
      macos_system.rs  # ScreenCaptureKit (or shells the signed .app helper)
      macos_mic.rs     # cpal
      windows_system.rs# wasapi loopback
      windows_mic.rs   # cpal/wasapi
  native/macos/BlueyAudio.app   # the SIGNED .app bundle for system capture (the real fix)
```
Bluey's daemon depends on `cue-transcribe` and consumes its transcript stream —
the daemon stops owning STT/capture internals.

## 5. Build order (one clean pass, verify each step before the next)
1. Scaffold `cue-transcribe`; copy the prototype's Nemotron streaming engine; prove
   it transcribes a WAV (we already measured this works: 0.12x RTF, correct text).
2. macOS mic capture (cpal) → live transcript. Simplest real capture.
3. macOS system capture: build the **signed `.app`** helper (the permission fix).
   Verify it appears in Screen Recording settings + captures non-silent audio.
4. Windows system (wasapi loopback) + mic. Research-backed; build + verify on Win.
5. Wire into Bluey daemon; feed the existing UI. Save transcript (clean).

## 5b. Progress (2026-06-29)
- ✅ `cue-transcribe` crate built + proven (transcribes WAV; 0.12x RTF).
- ✅ `BlueyAudio.app` — system helper as a grantable, signed `.app` bundle
  (`native/macos/cue-audio/bundle-app.sh`). The fix for the silent-capture wall:
  a bare CLI can't hold the Screen Recording TCC grant; a `.app` can.
- ✅ Daemon auto-discovers `BlueyAudio.app` (`system_capture.rs::platform_binary_path`)
  and transcribes live system audio, labeled `[system]` — PROVEN.
- ✅ Daemon ASR migrated to `cue-transcribe::SttEngine` (one engine; the old
  `parakeet.rs` is now a thin SttProvider adapter over it; sortformer diarization
  kept). 231 tests pass.
- ✅ Build/install ship the `.app`: `scripts/build-macos.sh` bundles + stages it;
  `install.sh` copies it (recursive); `run-local.sh` dev-installs so `bluey on`
  uses the latest build. Capture starts on the overlay "Listen" button.
- ⏳ Signing: ad-hoc for now (grant resets on rebuild). Real persistence needs an
  Apple Developer cert via `BLUEY_CODESIGN_IDENTITY` (user-provided).
- ⏳ TODO: mic capture (Step 2); Windows wasapi loopback (Step 4).

### Cross-platform caveat (flagged during migration)
On **Intel macOS**, the daemon overrides `parakeet-rs` to `load-dynamic` while
`cue-transcribe` requests `ort-defaults` (prebuilt). Cargo feature unification
would enable both on that target → potential link conflict. arm64 unifies cleanly
on `ort-defaults` (verified). Resolve before an Intel build (align both crates on
one ort linking mode per target).

## 6. Definition of done
- One `cargo build` per target produces a working binary.
- System + mic transcribe live, labeled, accurate, on-device.
- macOS grant survives rebuilds (stable signing).
- No webview capture, no dead code, lean.

## Sources (research, 2026-06-29)
- wasapi loopback (Direction::Capture, ShareMode::Shared, poll not event): https://docs.rs/wasapi , https://github.com/HEnquist/wasapi-rs
- cpal WASAPI loopback is unreliable (PR added then removed): https://github.com/RustAudio/cpal/issues/476
- ort execution providers (CoreML/DirectML/CPU availability): https://ort.pyke.io/perf/execution-providers
- parakeet-rs (CoreML unstable, CPU faster than whisper-metal; 560ms streaming): https://github.com/altunenes/parakeet-rs
- macOS: bare CLI can't appear in Screen Recording; .app bundle does; ad-hoc signing wipes grant on rebuild; CGRequestScreenCaptureAccess registers the app: https://developer.apple.com/documentation/screencapturekit/ , https://dgrlabs.co/blog/2026-04-25-capturing-system-audio-on-macos-in-2026.html

## Word-Drop / Word-Scramble Bug — Root Cause & Fix (in-order serialized sink)

> Diagnosed 2026-07-03 with real-time-streamed VoxConverse audio (raw STT engine
> vs daemon layer). Documented so the regression is not reintroduced.

### Symptom
Live system-audio transcript scrambled word runs ("... when I think for me, ...")
and silently dropped distinct finals under load — while STT had been seamless before.

### Root cause (NOT the STT engine)
The Nemotron/Parakeet engine is content-clean: real-time-paced and batch
transcripts are **byte-identical** (verified via `realtime_emit` vs a batch pass,
20 emits, IDENTICAL diff). The engine neither drops nor scrambles words.

The bug was in the daemon. Commit `5f211b0` replaced the in-order `.await` sink
(as in `8d6efbc:app.rs:1137`) with a **detached per-segment `tokio::spawn`**. On the
multi-thread tokio runtime (`#[tokio::main]`, no flavor), those tasks run
concurrently and, because the sink yields (`daemon.audio.lock().await`) BEFORE
taking the meeting lock, a later segment could `push` before an earlier one:
  - **Word-SCRAMBLE:** transcript committed out of model order.
  - **Word-DROP (downstream):** the reordered tail poisoned
    `is_near_duplicate_transcript` (walks the last 8 segments in an 8s window),
    which then discarded legitimate distinct finals as duplicates.

Separately, the engine is over real-time at the daemon's 20ms (320-sample) chunk
cadence (RTF 1.29x, worst push 128ms) but comfortably real-time at 100ms
(1600-sample) chunks (RTF 0.40x). Because `sys_rx` and the provider queue are
unbounded end to end, this deficit causes **unbounded latency growth, not word
loss** — a distinct problem from the scramble/drop.

### Fix
1. **In-order serialized sink (correctness — shipped).** One background consumer
   task per capture owns an unbounded mpsc receiver and awaits
   `add_audio_transcript_segment_allowing_session_start` for each segment in strict
   FIFO order. The `select!` loop hands each Final off with a non-blocking
   `seg_tx.send(...)`, so audio intake never blocks AND commit order == receipt
   order. On loop break: `drop(seg_tx); sink_task.await;` flushes in order. This
   preserves the `5f211b0` goal (sink I/O off the audio loop) while restoring the
   `8d6efbc` ordering invariant the dedup logic depends on.
2. **~100ms (1600-sample) chunk coalescing before `send_audio` (recommended,
   throughput only).** Keeps the engine under real-time; does not affect content.
   [Not yet applied — latency only, no word loss.]

### Invariant — do NOT reintroduce the regression
Transcript segments MUST be committed to `meeting.transcript` in the exact order
they arrive from the provider. **Never spawn one detached task per segment for the
sink** — that reorders commits on the multi-thread runtime and corrupts the dedup
tail. Any off-loop sink must be a SINGLE ordered consumer.

### Verification
- Raw engine: `realtime_emit` (20ms real-time) vs batch → byte-identical (PASS).
- Daemon: `STT_RECV_SEQ` labels each segment at arrival; the ordered consumer
  commits in that order. Verified live-streamed msbyq (44 segments): arrival seq
  `0..43` == commit seq `0..43`, monotonic, no gaps → no scramble, no drop.

## Live STT Lag + Eaten Words — Root Cause & Fix (chunk coalescing + VAD)

> Diagnosed 2026-07-03 by mapping the live path + web research + measuring RTF and
> queue backlog on real-time-streamed VoxConverse audio. Two DISTINCT bugs.

### Symptom
Live transcript increasingly LAGGED behind speech, and quiet/onset words were EATEN
(missing in the middle) — even with diarization OFF.

### Cause 1 — LAG: 20ms feed → redundant mel recompute → RTF>1 → unbounded backlog
The capture path frames 20ms/320-sample chunks and fed them 1:1 to the engine.
parakeet-rs recomputes the mel spectrogram over its WHOLE internal buffer on EVERY
`transcribe_chunk` call (nemotron.rs:628) — even the ~27/28 calls that early-return
without running the encoder. At 20ms cadence that's ~50 full-window mel recomputes/
sec → **RTF 1.29x** (over real-time). Every live queue is unbounded (sys_tx,
audio_tx, seg_tx), so an RTF≥1 worker can't drop or backpressure — the `audio_rx`
backlog grows **monotonically → ever-increasing lag**.

**Fix: coalesce forwarded frames to ~100ms (1600 samples) before `send_audio`**
(app.rs). Cuts mel recomputes ~5x → **RTF 0.25x measured**, backlog pinned at 0.
The engine self-buffers to its 560ms encoder window regardless, so transcript
content is UNCHANGED — verified byte-complete after coalescing. MEASURED on
real-time-streamed aepyx (169s): chunk 20ms→100ms, RTF 1.29→0.25, backlog
end=0/max=11 (was growing unbounded), transcript complete.

### Cause 2 — EATEN WORDS: RMS VAD gate dropped quiet / onset speech
The default build uses only the Send-safe `RmsGate`, which Drops chunks below 2%
RMS after 500ms silence. Quiet speech / soft talkers / utterance onsets after a
pause fell below the 2% floor and were dropped, clipping words.

**Fix: lower default `rms_threshold_start` 0.02→0.01 and raise
`silence_hangover_frames` 25→50 (1000ms)** (cue-core/src/vad.rs). Admits quiet
speech, holds longer after speech so trailing words aren't cut. Env-tunable via
`BLUEY_VAD_RMS_THRESHOLD` / `BLUEY_VAD_HANGOVER_MS`.

### Diagnostics added (to prove it live)
`STTPERF push` in the parakeet worker logs per-chunk `rtf` + `backlog` (climbing
backlog + RTF≥1 = lag bug); `vad_dropped` counter in the audio loop (high count on
talky audio = VAD eating speech).

### Invariant
Feed the STT engine ~100ms chunks, NOT 20ms — 20ms multiplies parakeet-rs's
per-call mel recompute ~5x and pushes RTF over real-time. Coalescing is content-
neutral (engine buffers to 560ms internally).

## Bursty / Laggy Live Transcript — Root Cause & Fix (continuous uniform feed)

> Diagnosed 2026-07-03. Text arrived in multi-second BURSTS (86 clumps, gaps up to
> ~9s) despite the STT queue keeping up (backlog 0, RTF 0.26x) and 0 VAD drops —
> i.e. the lag/eating was NOT queue backpressure. The user's instinct was right:
> audio was being LOST/DISRUPTED in the gaps between sends.

### Two mistakes we made (both in the daemon audio→engine feed, app.rs)
1. **VAD-gated the engine feed.** The RMS VAD dropped "silence" frames BEFORE the
   engine via `continue`. But Parakeet/Nemotron is CACHE-AWARE STREAMING and
   assumes a CONTINUOUS audio timeline. Dropping frames punches HOLES in that
   timeline → (a) any misjudged-quiet-speech is permanently lost, (b) the streaming
   cache desyncs → output batches into multi-second bursts.
2. **Fed VARIABLE-size chunks.** The coalescing sent ~100ms during speech but
   short 20ms flushes on silence boundaries / timeouts. Irregular chunk sizes also
   desync the streaming window, worsening the bursting.

### The fix (both required)
- **NO VAD gating on the system-audio engine feed.** Feed the model everything,
  continuously. It handles silence itself. (VAD may still be used mic-side later,
  but never to gate the streaming engine.)
- **UNIFORM fixed-size chunks.** Buffer to EXACTLY `COALESCE_SAMPLES` (1600 = 100ms
  @16k) and only ever `send_audio` that size, carrying the remainder. Every chunk
  the engine sees is identical → the cache stays in sync.
  MEASURED: bursts 86→9, max gap 9.0s→3.4s, median emit ~593ms (matches the model's
  ~560ms window), RTF 0.26x, backlog flat. Remaining >3s gaps = genuine audio silence.

### The full STT invariant set (do NOT reintroduce any of these regressions)
1. **Feed a CONTINUOUS, UNIFORM stream** to the cache-aware streaming engine — no
   VAD holes, no variable chunk sizes. Fixed ~100ms chunks including silence.
2. **~100ms chunks, never 20ms.** 20ms multiplies parakeet-rs's per-call mel
   recompute ~5x → RTF over real-time → unbounded backlog → growing lag.
3. **Commit transcript segments IN RECEIPT ORDER** via a single ordered sink task —
   never one detached `tokio::spawn` per segment (reorders on the multi-thread
   runtime → scrambled/dropped words).
4. **Never do disk I/O (`save_active`) under the `meeting` lock** — it blocks the
   STT sink 50-200ms. Clone under the lock, save after releasing it.
5. STT runs **CPU (ort), not CoreML** — CoreML is unstable for this model.

### Diagnostic tooling left in place
- `STTPERF push` (parakeet worker): per-chunk `rtf` + `backlog`. RTF≥1 + climbing
  backlog = lag. `RUST_LOG=cue_daemon=debug` to see it.
- Emit-cadence check: cluster committed segments by `created_at`; many bursts with
  big gaps (with backlog 0 + 0 VAD drops) = streaming-cache desync, not queue lag.
- WAV harness: `BLUEY_AUDIO_WAV_FILE=<16k mono wav>` drives the full live pipeline
  at real-time cadence for reproducible measurement (crates/cue-transcribe examples
  realtime_emit/chunk_timing for raw-engine A/B).

## CORRECTION: the bursty emit is INTRINSIC to the model (not chunk sizing / VAD)

> A follow-up diagnosis (real-time-streamed aepyx, fixed-20ms vs fixed-100ms vs
> variable-coalesced feeds) OVERTURNED the "continuous uniform feed fixed the
> bursting" conclusion above. Correcting it here so we don't chase chunk sizing.

**Measured (227 emits each, stream-time emit gaps):**
| feed | median | max |
|---|---|---|
| fixed 20ms | 0.560s | 3.36s |
| fixed 100ms | 0.600s | 3.40s |
| variable coalesced | 0.600s | 3.38s |

**Byte-identical.** Chunk size / VAD gating / coalescing make ZERO difference to
emit timing. The earlier "86→9 bursts" was run-to-run noise, not a fix.

**Real root cause:** parakeet-rs `transcribe_chunk` only runs the encoder every
56 mel frames (=560ms, `available_new_frames < CHUNK_SIZE` gate, nemotron.rs:637),
AND the greedy streaming RNN-T decoder emits BLANK across several consecutive
560ms windows while a word/phrase resolves, then commits a run of tokens at once.
`push` returns `None` on empty text → no segment → then a burst. This is inherent
to greedy streaming RNN-T decoding on this model; the big gaps are NOT silence
(measured RMS in the gap windows ≈ non-silent).

**What the earlier fixes DID legitimately fix (keep them):**
- 100ms coalescing → RTF 1.29x→0.25x (real CPU/lag win, content-neutral).
- Ordered single sink → in-order commits (real scramble/drop fix).
- Off-lock save → STT ∥ diarization (real fix).
These are correct. Only the "uniform feed removes bursting" claim was wrong.

**To actually reduce perceived bursting (future, NOT chunk sizing):**
1. Emit PARTIAL / word-level hypotheses, not just committed tokens.
2. Client-side progressive reveal / smoothing of a burst's tokens.
3. A shorter model streaming config than 56 frames (model-export change).
The VAD gate is still worth removing for a DIFFERENT reason (silence frames drop
audio → real inter-utterance gaps stretch wall-clock further), but it is NOT the
source of the intrinsic burst.

## Live diarization: multi-speaker passages lumped on one id — Fix (full-audio re-diarize)

### Symptom
Long stretches of multi-speaker dialogue were all labelled one speaker (e.g. 58
consecutive segments = "Speaker 0"), even though several people were talking.

### Root cause
The live tier re-diarized only a 30s ROLLING window. A segment gets labelled at
the tick when it's still in-window — but early on only ONE speaker has spoken, so
it's labelled speaker 0. Once the rolling window scrolls past that segment, later
ticks (which now see more speakers) can never re-label it — its
`audio_start_secs` no longer overlaps the current window's turns. So early
segments FREEZE at whatever id they got before other speakers appeared.

### Fix
The live tier now re-diarizes the FULL meeting audio (from t=0) each tick, not a
30s rolling window (`live_tick` submits `retention.full()`, start=0). The whole
timeline is always present, so `label_segments_by_overlap` (which overwrites every
segment each tick) re-labels EARLY segments correctly as more speakers appear.
The LiveDiarizer's time-overlap-to-previous id mapping still gives stable arrival-
ordered ids. Runs on the diarizer's dedicated thread → the growing per-tick cost
(speakrs over the whole meeting) never touches STT. MEASURED on aepyx (4-speaker,
90s): distribution {0:75,1:26,2:3} → {0:34,1:49,2:22}; longest single-speaker run
58→22 segments. The meeting-end post pass remains authoritative.
