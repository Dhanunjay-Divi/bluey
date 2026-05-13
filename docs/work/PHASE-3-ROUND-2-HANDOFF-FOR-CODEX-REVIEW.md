# Phase 3 Round 2 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-2`
**Base**: `2ac342a` (main post-Round-1-merge + review recording)
**Tip**: `4091092`
**Author**: kiro

## Scope

Round 2 of Phase 3. Concrete capture + VAD + MockStt implementations on top of the Round-1 type foundation. All 4 follow-ups from codex's Round-1 review addressed (3 fixed, 1 carried to Round 3 intentionally).

### What shipped

1. **CPAL microphone capture** (`crates/cue-daemon/src/audio/capture.rs`) — dedicated thread, i16+f32 sample formats, mono downmix, emits framed chunks via tokio channel
2. **Framer** (`crates/cue-daemon/src/audio/framer.rs`) — ring-buffered chunk accumulator, 20 ms fixed output, `flush_padded` for graceful shutdown, `f32_to_i16` + `downmix_to_mono` DSP helpers
3. **Two-stage VAD** (`crates/cue-daemon/src/audio/vad.rs`) — `RmsGate` (adaptive, tracks noise floor) → `WebRtcGate` (real `webrtc-vad` crate) → `TwoStageVad` orchestrator with documented decision matrix
4. **MockStt provider** (`crates/cue-daemon/src/stt/mock.rs`) — implements `SttProvider`, paired with `MockSttControl` test handle, 6 unit tests
5. **Pipeline integration tests** (`crates/cue-daemon/tests/pipeline_integration.rs`) — 4 end-to-end tests proving the capture-shaped→VAD→STT contract
6. **delete_session persistence tightened** — lock failures no longer bubble as user-facing errors
7. **tokio-tungstenite pruned** — reserved dep with zero current usage; re-add when Deepgram lands in Round 3

## Commits

```
4091092 style: cargo fmt post-integration [P3.R2]
9ceceb2 feat(daemon): audio framer + two-stage VAD + MockStt + pipeline integration test [P3.R2]
c99f6ea fix(dashboard): relax delete_session persistence errors + prune unused tokio-tungstenite dep [P3.R2]
```

Plus IMPL + this handoff doc committed next.

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets --release                  ✅ 49.36s
cargo test --all-targets                             ✅ 97 pass
cd crates/cue-dashboard/ui && npm run build          ✅ 297 KB JS (94 KB gz)
git diff --check main..HEAD                          ✅ clean
python3 tomllib .codex/agents/*.toml                 ✅ all 7 valid
```

Test count:

| Where | Round 1 | Round 2 | Δ |
|---|---|---|---|
| cue-core lib | 45 | 45 | — |
| cue-daemon lib | 23 | 48 | +25 (10 framer, 10 vad, 6 mock-stt — minus 1 dup-test removed as part of app_state inlining) |
| Integration tests | 0 | 4 | +4 (new file `tests/pipeline_integration.rs`) |
| **Total** | **68** | **97** | **+29** |

Plus 1 ignored hardware-gated test for the real CPAL capture.

## Follow-ups carried to Round 3

- **Integration test proving `SessionSwitched` is emitted through the native-overlay pipe.** Requires the daemon-side overlay spawn + stdin piping (also Round 3 work). Left as an explicit Round-3 task.

## Architecture diagram (audio path)

```
CPAL callback (variable-size, i16 or f32, device channels)
  │
  ▼
[audio::capture] — thread-owned CPAL stream
  │  convert f32→i16 if needed
  │  downmix to mono
  ▼
[audio::framer::Framer] — ring buffer, 20 ms chunks
  │  emits AudioChunk { source, sample_rate, samples, captured_at_ms }
  ▼
[audio::vad::TwoStageVad]
  │  Stage 1 RmsGate (adaptive noise floor)
  │    Drop      → drop
  │    Send      → ask Stage 2
  │    SendSilence → forward (skip Stage 2)
  │  Stage 2 WebRtcGate
  │    true      → Send
  │    false     → SendSilence
  ▼
[Any SttProvider impl — MockStt today, Deepgram next round]
  │  send_audio / finalize / close
  │  emits TranscriptEvent::{Partial,Final} via next_event
  ▼
[Future: Tauri event → React `useSessionEvents`-style consumer]
```

## Design highlights (for reviewer context)

### VAD decision matrix
```
RmsGate          | WebRtcGate     | FrameAction
-----------------+----------------+------------
Drop             | (skipped)      | Drop
Send             | true           | Send
Send             | false          | SendSilence
Send             | error          | Send  (fail open)
SendSilence      | (skipped)      | SendSilence
```

### MockStt split
- `MockStt: SttProvider` — handed to pipeline, implements the trait
- `MockSttControl` — retained by tests, scripts provider behavior + observes state

### Capture threading
- Dedicated OS thread owns the CPAL stream (CPAL requirement)
- Tokio channel bridges thread → async consumer
- `Arc<AtomicBool>` stop signal + join on drop

## Dependency changes

Added to `cue-daemon`:
- `cpal`, `ringbuf` (prewired), `webrtc-vad`, `bytemuck`, `async-trait`, `parking_lot`, `futures-util`, `thiserror`
- `tokio` was already there

Pruned from workspace:
- `tokio-tungstenite` — reserved for Deepgram Round 3; re-add then

## Known quirks

1. **Ignored hardware test** (`capture.rs::capture_starts_and_stops_cleanly`) — runs only with real input device.
2. **WebRTC VAD on synthetic waveforms** is unreliable — tests accept both `Send` and `SendSilence` for loud synthetic audio. Real speech in Round 3 integration test will exercise it fully.
3. **`cue-daemon::audio` does NOT re-export the chunk type** — `AudioChunk` stays in `cue_core::pcm` and is imported where needed. Keeps the audio module focused on runtime behavior.
4. **Clippy fixes applied in this round**: useless `vec!` in tests → array literal, redundant `as u64` cast, `repeat().take()` → `repeat_n`, field-after-default-init pattern replaced with functional update.

## Review checklist for codex

### Correctness
- [ ] `Framer::push` with 1000 samples @ 20 ms / 16 kHz emits 3 chunks (960 samples) and retains 40 samples
- [ ] `Framer::flush_padded` on empty buffer returns `None`, not an empty chunk
- [ ] `Framer::f32_to_i16` clamps values above +1.0 and below -1.0 to ±i16::MAX (symmetric)
- [ ] `Framer::downmix_to_mono` drops trailing partial frames (e.g., 5-sample stereo buffer yields 2 mono samples, last sample dropped)
- [ ] `RmsGate` adaptive threshold only raises, never lowers (threshold tracks `noise_floor * 3.0` with a minimum of 0.01)
- [ ] `RmsGate` noise floor only updates on sub-threshold frames (speech doesn't bias the floor)
- [ ] `WebRtcGate::new` rejects non-8/16/32/48 kHz rates with an error that names the offending value
- [ ] `WebRtcGate::is_speech` rejects chunks that aren't 10/20/30 ms
- [ ] `TwoStageVad::process` short-circuits on `Drop` (doesn't ask WebRTC)
- [ ] `MockStt::send_audio` increments both chunk count and byte count
- [ ] `MockStt::send_audio` after `close` returns `NotActive`, not a silent success
- [ ] Integration test `pipeline_silence_is_dropped_before_stt` correctly asserts ≤1 chunk forwarded (accounting for hangover)

### Follow-ups
- [ ] `tokio-tungstenite` line absent from `Cargo.toml`
- [ ] `Cargo.lock` reflects the removal (no tungstenite in the dep graph)
- [ ] `delete_session` in `commands.rs` uses `match db.0.lock() { Ok(...) => ..., Err(e) => tracing::warn!(...) }` — not `?`
- [ ] Comments in `delete_session` explain why the error is swallowed (self-heal-on-startup guarantee)

### Style / hygiene
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy -- -D warnings` clean
- [ ] No `unwrap()` added in non-test code beyond preexisting pattern
- [ ] Module-level doc comments on framer.rs, vad.rs, capture.rs, mock.rs explain intent
- [ ] Integration tests have descriptive function names that state what they prove

### Next-round readiness
- [ ] `SttProvider` trait shape handles streaming WS → `Deepgram::send_audio` can wrap this; `next_event` polls WS receiver
- [ ] `AudioChunk.captured_at_ms` plumbs through unchanged (no resets or overwrites in framer/VAD)
- [ ] `TwoStageVad::process` is `&mut self` and `Send` — composable into tokio task
- [ ] `MockSttControl::emit_partial/emit_final/emit_error` cover what Deepgram integration tests will need

## Verdict request

Codex: review the 3 commits + new modules + integration test + IMPL doc. Write `docs/work/REVIEW-PHASE-3-ROUND-2.md`.

- 🟢 ACCEPT → merge to main, start Round 3 (Deepgram Nova-3 WS provider + daemon↔overlay IPC wiring + end-to-end overlay SessionSwitched integration test)
- 🟡 ACCEPT WITH NITS → I fold nits into Round 3
- 🔴 REQUEST CHANGES → I write `FIX-PHASE-3-ROUND-2.md` using `TEMPLATE-FIX.md`
