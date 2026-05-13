# IMPL — Phase 3 Listening Upgrade (Round 2 — Capture, VAD, MockStt)

**Branch**: `feat/phase-3-round-2`
**Base**: `2ac342a` (main post-Round-1-merge + codex review recording)
**Tip**: `4091092`

## Scope

Round 2 of Phase 3. Concrete implementations now land on top of the type foundations from Round 1:

- **CPAL microphone capture** — platform-aware (macOS CoreAudio / Windows WASAPI / Linux ALSA via CPAL), device format conversion to i16 mono, dedicated capture thread with tokio channel to rest of pipeline
- **Ring-buffered chunking via `Framer`** — handles variable-size CPAL callbacks, emits fixed 20 ms chunks, pads trailing partial frame on flush
- **RMS adaptive gate** — stage 1 of the VAD pipeline, cheap, tracks running noise floor
- **WebRTC VAD wrapper** — stage 2, real `webrtc-vad` crate, validates frame-size + sample-rate compatibility
- **`TwoStageVad`** — orchestrates the two stages with a documented decision matrix
- **`MockStt` provider** — implements `SttProvider` against scripted `MockSttControl`, enables integration tests without network/hardware
- **Integration test suite** — 4 tests proving the full capture-shaped → VAD → STT pipeline matches the contracts

## Commits

| Hash | Title |
|---|---|
| `c99f6ea` | `fix(dashboard): relax delete_session persistence errors + prune unused tokio-tungstenite dep [P3.R2]` |
| `9ceceb2` | `feat(daemon): audio framer + two-stage VAD + MockStt + pipeline integration test [P3.R2]` |
| `4091092` | `style: cargo fmt post-integration [P3.R2]` |

## Carried follow-ups from Codex's Round 1 review — all resolved

1. **✅ Consumed pre-wired deps:** `cpal`, `ringbuf`, `webrtc-vad`, `bytemuck`, `async-trait`, `parking_lot`, `futures-util`, `thiserror` all now imported by `cue-daemon`. **Pruned `tokio-tungstenite`** from the workspace — it was reserved for Deepgram (Round 3) but carried no value in this round's Cargo.lock. Will re-add in Round 3 when the Deepgram WebSocket provider lands.
2. **✅ Tightened `delete_session` persistence:** lock acquisition failures no longer bubble as `Result::Err` to the user. The delete's in-memory clear already succeeded; persistence is best-effort logged warn. Verified by running through the code path and reviewing control flow.
3. **✅ Concrete MockStt + VAD/capture tests before Deepgram lands** (this round — see tests section below).
4. **Deferred to Round 3+**: integration test proving `SessionSwitched` is emitted through the native-overlay pipe. That needs the overlay-spawn wiring first, which is Round 3.

## Files created / modified

| File | Change |
|---|---|
| `Cargo.toml` | Pruned `tokio-tungstenite` workspace dep (unused until Round 3) |
| `crates/cue-dashboard/src/commands.rs` | `delete_session` persistence now matches on `db.lock()` instead of `?`, logs on failure |
| `crates/cue-daemon/Cargo.toml` | Added cpal, ringbuf, webrtc-vad, bytemuck, async-trait, parking_lot, futures-util, thiserror |
| `crates/cue-daemon/src/audio/mod.rs` | New module — public `framer`, `vad`, `capture` |
| `crates/cue-daemon/src/audio/framer.rs` | `Framer` (push/flush/pending) + `f32_to_i16` + `downmix_to_mono` helpers + 10 unit tests |
| `crates/cue-daemon/src/audio/vad.rs` | `RmsGate`, `WebRtcGate`, `TwoStageVad` + `normalized_rms` helper + 10 unit tests |
| `crates/cue-daemon/src/audio/capture.rs` | `MicrophoneCapture` (dedicated-thread CPAL stream, i16+f32 paths, mono downmix) + 1 ignored hardware test |
| `crates/cue-daemon/src/stt/mod.rs` | New module — exports `mock` |
| `crates/cue-daemon/src/stt/mock.rs` | `MockStt` (implements `SttProvider`) + `MockSttControl` (test-side driver) + 6 async unit tests |
| `crates/cue-daemon/src/lib.rs` | `pub mod audio; pub mod stt;` |
| `crates/cue-daemon/tests/pipeline_integration.rs` | 4 end-to-end tests — silence dropped, loud audio passes, connection state transitions, finalize+close |

## Build + test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass (addressed useless_vec, same-type cast, repeat/take idiom, field-init-after-default)
cargo build --all-targets --release                  ✅ 49.36s
cargo test --all-targets                             ✅ 97 pass (45 core + 48 daemon lib + 4 integration) 0 failed, 1 ignored (hardware capture test)
cd crates/cue-dashboard/ui && npm run build          ✅ 297 KB JS
git diff --check main..HEAD                          ✅ clean
```

Test count change:
- Round 1 end: 68 total (45 core + 23 daemon)
- Round 2 end: 97 total (45 core + 48 daemon lib + 4 integration)
- **+29 tests** (25 new in daemon lib — framer 10, vad 10, mock_stt 6 (minus 1 unused dedupe); 4 in integration)

## Design decisions worth calling out

### Framer
- **Timestamps advance per emitted chunk** (head + duration) rather than tracking each sample's arrival. Good enough for latency instrumentation and simpler to reason about.
- **`flush_padded` pads with zeros** rather than returning whatever's buffered. VAD and STT both expect exactly-sized frames; padding is cheaper than teaching downstream to tolerate short frames.
- **`downmix_to_mono` drops trailing partial frames** rather than erroring. CPAL callback buffers can end on a partial-frame boundary; the next callback catches the remainder. Dropping keeps the function pure and easy to test.

### RmsGate
- **Adaptive threshold** tracks `noise_floor * 3.0` with a minimum of 0.01. Clamped so the gate never zeros out against very quiet microphones.
- **EMA coefficient `noise_alpha = 0.05`**: ~20-frame (400 ms) time constant for noise-floor tracking. Fast enough to adapt when the user starts speaking in a new environment; slow enough not to chase speech envelope.
- **Noise floor only updates on frames BELOW threshold.** During speech, the floor freezes. Prevents the gate from auto-muting during continuous speech.

### WebRtcGate
- **Validates sample rate + frame duration up-front.** Returns an Err that explains what it needs. Prevents silent wrong-results when a future pipeline change emits 15 ms frames or 44.1 kHz.
- **Fails open on runtime error** (`Ok(Ok(true))` vs `Ok(Ok(false))`)-style — if `webrtc-vad` rejects a frame, we fall back to the RMS decision rather than dropping the whole chunk. Logged for diagnosis.

### TwoStageVad decision matrix
- **RMS says Drop → Drop** (skip WebRTC). Fast path; most silent frames never reach the VAD.
- **RMS says Send → ask WebRTC**. If WebRTC confirms → Send; if WebRTC denies → SendSilence (don't burn STT quota on fan noise that's loud enough to pass RMS).
- **RMS says SendSilence → SendSilence** (already in hangover window, skip WebRTC).

### MockStt
- **Split `MockStt` (provider) from `MockSttControl` (test handle).** Provider is handed to pipeline code as `Box<dyn SttProvider>` or direct; control is retained by the test. This pattern scales to real tests without exposing internal state.
- **`parking_lot::Mutex` not `tokio::sync::Mutex`** for the shared state — lock holds are tiny (set a flag, increment a counter) and parking_lot is faster. `events_tx` is still a tokio channel because tests need async receive.
- **`send_audio` after `close` returns `SttError::NotActive`** — matches what a real provider does when its socket closes.

### delete_session tightening
- **Lock acquisition errors are no longer fatal for delete_session.** The DB write already committed before we try to update persisted active-session state. A lock failure here must not turn a successful delete into an error the dashboard shows to the user.
- **`load_active_session` validates on startup** — so even if the persisted value gets out of sync (which can happen if the persistence hook failed AND the daemon crashed before self-healing), the next startup filters out stale ids.

## Known quirks

1. **One ignored test** (`crates/cue-daemon/src/audio/capture.rs::tests::capture_starts_and_stops_cleanly`). Requires real audio hardware + microphone permission; CI runners don't have that. Marked `#[ignore]`. Run on a developer machine via `cargo test -p cue-daemon -- --ignored`.
2. **WebRTC VAD on synthetic waveforms** — the tests use a triangle ramp as "loud audio" which WebRTC often classifies as non-speech. `pipeline_loud_audio_passes_to_stt_and_delivers_scripted_transcript` only asserts chunks pass RMS; it accepts both `Send` and `SendSilence` as valid outcomes because WebRTC's neural net is hard to fool with synthetic signals. Real speech (Round 3 E2E test) will exercise both stages correctly.
3. **CPAL capture is single-device today** — picks the system default input. Device selection UI and enumeration are Round 3 work.
4. **`cue-daemon` now depends on `parking_lot`** directly; previously only via transitive. Explicit dependency is safer.

## Not in scope for Round 2

- **Real Deepgram Nova-3 provider** (Round 3): persistent WS, auth, reconnect with backoff, NDJSON frame decoding, word-level timing
- **System audio capture** on macOS (ScreenCaptureKit) + Windows (WASAPI loopback) (Round 4)
- **Daemon ↔ native-overlay IPC wiring** — `OverlayMessage` types exist; spawn + stdin pipe is Round 3
- **Real Swift/C overlay updates** to consume `SessionSwitched` (Round 4)
- **End-to-end integration test for overlay SessionSwitched** (carried — Round 3 prerequisite)

## Review checklist for codex

### Audio correctness
- [ ] `Framer::push` emits chunks only when `buffer >= chunk_samples`; partial buffer retained
- [ ] `Framer::push` with large input yields multiple chunks in one call, with timestamps advancing by chunk duration
- [ ] `Framer::flush_padded` pads with zeros, returns original-content-first followed by zeros
- [ ] `Framer::flush_padded` returns `None` for empty buffer (no spurious zero chunk)
- [ ] `f32_to_i16` uses `i16::MAX` scaling (symmetric ±32767), clamps out-of-range values
- [ ] `downmix_to_mono` averages all channels, drops trailing partial frame
- [ ] `downmix_to_mono` mono-input passthrough is optimal (no allocation)

### VAD correctness
- [ ] `normalized_rms` returns 0.0 for empty input (no `/0`)
- [ ] `normalized_rms` uses `i64` accumulator to avoid overflow on long chunks
- [ ] `RmsGate` resets silence counter on speech frame
- [ ] `RmsGate` threshold only increases, never decreases (adaptive ratcheting up for noisy environments)
- [ ] `RmsGate` noise floor only updates on sub-threshold frames (prevents chasing speech envelope)
- [ ] `WebRtcGate::new` rejects 22050 Hz / other non-standard rates with clear error
- [ ] `WebRtcGate::is_speech` rejects 50 ms chunks with clear error
- [ ] `TwoStageVad` short-circuits on `FrameAction::Drop` (no WebRTC call)
- [ ] `TwoStageVad` downgrades `Send` → `SendSilence` when WebRTC says "not speech"
- [ ] `TwoStageVad` fails open if WebRTC errors (keeps pipeline running)

### MockStt correctness
- [ ] `SttProvider::send_audio` returns `NotActive` after `close()`
- [ ] `SttProvider::next_event` yields events in emission order (verified by partial→partial→final test)
- [ ] `MockSttControl` counters (chunks_received, bytes_received, was_finalized, was_closed) reflect provider activity correctly
- [ ] `set_connection_state` is visible via provider's `connection_state()`
- [ ] Scripted errors (`emit_error(SttError::Auth)`) round-trip through `next_event` with `should_failover()` true

### Integration test correctness
- [ ] `pipeline_silence_is_dropped_before_stt` asserts ≤1 chunk forwarded (not == 0, because hangover frame is valid)
- [ ] `pipeline_loud_audio_passes_to_stt_and_delivers_scripted_transcript` doesn't assume specific WebRTC classification of synthetic waveform
- [ ] `pipeline_finalize_and_close_round_trip` exercises all four provider methods in order
- [ ] All 4 tests are `#[tokio::test]` (async runtime required for await)

### Follow-ups applied
- [ ] `tokio-tungstenite` removed from `Cargo.toml` (verify via `grep tungstenite Cargo.toml` returns nothing)
- [ ] `delete_session` lock acquisition failure logs warn, does NOT return `Err` (trace through the control flow in `commands.rs`)
- [ ] Dashboard UI still builds after dep prune (no hidden transitive usage)

### Next-round readiness
- [ ] `MockStt` is shape-compatible with what Deepgram's streaming implementation will need (send_audio chunks, recv events, connection state, error classification)
- [ ] `Framer`'s 20 ms output is correct for both WebRTC VAD and most STT providers
- [ ] `MicrophoneCapture::sample_rate()` is queryable before `start()` returns (needed by Deepgram's `sample_rate` config field)
