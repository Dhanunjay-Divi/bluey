# IMPL — Phase 3 Listening Upgrade (Round 4 — Overlay Restart Loop + System Audio Capture)

**Branch**: `feat/phase-3-round-4`
**Base**: main tip (post-Round-3-merge, 130 tests)
**Tip**: `6fc8682` (4 commits ahead of main, 136 tests)

## Scope

**Does:**

1. Restructures the overlay supervisor to own `send_rx`, eliminating a restart-loop race condition that caused flaky test failures.
2. Implements the overlay restart-on-crash loop with exponential backoff and carryover semantics.
3. Adds a Windows overlay rendered with Direct2D (full native overlay UI) and Windows CI.
4. Adds system audio capture via native OS helpers (macOS ScreenCaptureKit, Windows WASAPI loopback) with a Rust launcher module that frames raw PCM into `AudioChunk`s.

**Does NOT:**

- Route system audio through the STT pipeline (follow-up).
- Implement the STT fallback chain (separate plan doc exists).
- Replace the existing mic audio pipeline — system audio is additive and opt-in.

## Commits

| Hash | Title | Author |
|------|-------|--------|
| `19ff43a` | `fix(daemon): restructure overlay supervisor to own send_rx, fixes restart-loop race [P3.R4 stage 1]` | kiro (subagent 1) |
| `6f21864` | `feat(windows): render overlay with Direct2D and add Windows CI` | uno (user) |
| `329f324` | `fix(windows): make native helper builds pass MSVC` | uno (user) |
| `6fc8682` | `feat(daemon): system audio capture via native helpers (macOS ScreenCaptureKit + Windows WASAPI) [P3.R4 stage 2]` | kiro (subagent 2) |

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/overlay.rs` | Modified | Restructured supervisor to own `send_rx`; added `run_one_child` + `run_supervisor` restart loop with carryover semantics |
| `crates/cue-daemon/src/bin/overlay_stub_oneshot.rs` | Created | Stub that reads one message, sends Pong, then exits non-zero (simulates crash for restart test) |
| `crates/cue-daemon/tests/overlay_restart_integration.rs` | Created | Integration test: sends msg → gets Pong → child crashes → supervisor restarts → sends second msg → gets second Pong |
| `native/windows/cue-overlay/main.c` | Created | Full Windows overlay with Direct2D rendering, NDJSON stdin/stdout IPC, WS_EX_LAYERED + capture exclusion |
| `native/windows/cue-overlay/build.ps1` | Modified | Updated build script for MSVC compatibility |
| `.github/workflows/ci.yml` | Modified | Added Windows CI job |
| `crates/cue-dashboard/icons/icon.ico` | Modified | Updated icon |
| `native/windows/cue-audio/main.c` | Modified | MSVC compatibility fixes for WASAPI loopback helper |
| `native/windows/cue-audio/build.ps1` | Created | Windows audio helper build script |
| `scripts/build-windows.ps1` | Created | Top-level Windows build orchestration |
| `native/macos/cue-audio/Package.swift` | Created | Swift Package Manager manifest for macOS audio helper |
| `native/macos/cue-audio/Sources/cue-audio/main.swift` | Created (moved) | ScreenCaptureKit system audio + AVAudioEngine mic capture, outputs 16 kHz mono i16 LE on stdout |
| `native/macos/cue-audio/.gitignore` | Created | Ignores `.build/` directory |
| `native/macos/cue-audio/build.sh` | Modified | Updated for SPM structure |
| `crates/cue-daemon/src/audio/system_capture.rs` | Created | Rust launcher: spawns native helper, reads stdout PCM, frames into 20 ms `AudioChunk`s, restart-on-crash |
| `crates/cue-daemon/src/audio/mod.rs` | Modified | `pub mod system_capture;` |
| `crates/cue-daemon/src/bin/system_audio_stub.rs` | Created | Mock binary: outputs 2 s of 440 Hz sine as 16 kHz mono i16 LE (for integration tests) |
| `crates/cue-daemon/tests/system_audio_integration.rs` | Created | 2 integration tests: chunk reception + clean stop |
| `crates/cue-daemon/src/app.rs` | Modified | `BLUEY_SYSTEM_AUDIO_CONTINUOUS` env var opt-in wiring |
| `crates/cue-daemon/Cargo.toml` | Modified | Added `[[bin]]` entries for `overlay-stub-oneshot` and `system-audio-stub` |

## Design Decisions

### 1. Supervisor owns `send_rx` — fixes restart-loop race

**Problem:** In Round 3, the writer task held `send_rx` independently of the child process lifecycle. When the child crashed and the supervisor wanted to restart, there was a race: messages could be consumed from `send_rx` by the old writer task after the child's stdin was already closed, causing silent message loss and flaky test failures.

**Fix:** The supervisor now owns `send_rx` directly. `run_one_child()` takes ownership of `send_rx`, uses `tokio::select!` on `child.wait()` vs `send_rx.recv()`, and returns `send_rx` back to the supervisor when the child exits. This guarantees:
- No messages are lost between child generations.
- The `msgs_written` / `msgs_acked` counters track whether the last message was actually consumed by the child.
- If the child crashes before acknowledging the last written message, that message becomes "carryover" and is re-sent to the next child generation.

### 2. System audio capture via native helper processes (not Rust FFI)

Mirrors the existing overlay helper pattern: the daemon spawns a platform-specific binary and communicates via stdout. Rationale:
- ScreenCaptureKit requires Swift + Objective-C runtime; WASAPI requires COM initialization. Both are painful to bind via Rust FFI.
- Process isolation: a crash in the audio helper doesn't take down the daemon.
- The same restart-on-crash pattern (exponential backoff, `MAX_RESTART_ATTEMPTS = 5`) applies.

### 3. 16 kHz mono i16 LE on stdout as the helper-to-daemon ABI

The native helpers resample from whatever the OS provides (typically 48 kHz float) down to 16 kHz mono i16 little-endian, then write raw PCM bytes to stdout. The Rust `SystemAudioCapture` module reads this stream and frames it into 20 ms chunks (320 samples = 640 bytes each). This format:
- Matches what STT providers expect (Deepgram Nova-3 wants `linear16` at 16 kHz).
- Is trivial to parse (no container format, no headers).
- `--continuous` flag keeps the helper running indefinitely until killed.

### 4. Opt-in via `BLUEY_SYSTEM_AUDIO_CONTINUOUS` env var

System audio capture is gated behind `BLUEY_SYSTEM_AUDIO_CONTINUOUS=1`. This keeps the existing mic-only audio pipeline unchanged and avoids surprising users with loopback capture. The chunks flow into the daemon's existing `AudioChunk` channel but are not yet routed to STT — that's a follow-up.

### 5. macOS Swift helper — ScreenCaptureKit (macOS 13+ minimum)

Uses `SCStream` with `capturesAudio = true`, `excludesCurrentProcessAudio = true`. Captures at 48 kHz mono float, resamples to 16 kHz via a simple carry-based decimator in `PCM16Writer`. Also supports `--source microphone` mode via `AVAudioEngine` for future flexibility.

### 6. Windows C helper — WASAPI loopback, MSVC-compatible

Uses `IAudioClient` with `AUDCLNT_STREAMFLAGS_LOOPBACK` on the default render endpoint. Handles float32, int16, int24, and int32 PCM formats from the mix format. Resamples to 16 kHz via the same carry-based approach. Pure C with `COBJMACROS` for COM — compiles under both MSVC and MinGW.

### 7. Restart-on-crash for system audio child (exponential backoff)

`supervisor_loop` in `system_capture.rs` mirrors the overlay supervisor pattern: spawn → read until EOF/crash → if unexpected exit, increment failure counter, sleep `restart_delay(attempt)`, respawn. Gives up after `MAX_RESTART_ATTEMPTS = 5`.

## Tests Added

| Test | File | Type |
|------|------|------|
| `overlay_supervisor_respawns_on_unexpected_exit` | `tests/overlay_restart_integration.rs` | Integration |
| `restart_delay_exponential_and_capped` (system_capture) | `src/audio/system_capture.rs` | Unit |
| `chunk_constants_are_correct` | `src/audio/system_capture.rs` | Unit |
| `start_with_mock_binary` | `src/audio/system_capture.rs` | Async unit |
| `system_audio_capture_receives_chunks_from_stub` | `tests/system_audio_integration.rs` | Integration |
| `system_audio_capture_stops_cleanly` | `tests/system_audio_integration.rs` | Integration |

**Total: +6 tests (R3: 130 → R4: 136)**

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 136 pass
cd crates/cue-dashboard/ui && npm run build          ✅ pass
swift build (native/macos/cue-audio)                 ✅ pass
git -P diff --check main..HEAD                       ✅ clean
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Windows overlay + CI added by user (not originally in agent scope) | User contributed Direct2D overlay and Windows CI in parallel; integrated cleanly |
| System audio not yet routed to STT | Explicit deferral — the capture layer is complete; routing requires STT pipeline changes that are Round 5+ scope |

## Known Follow-ups

1. **STT routing for system audio** — `AudioChunk { source: System }` chunks currently flow into the daemon channel but are not forwarded to any STT provider. Requires a multiplexer or source-aware routing layer.
2. **STT fallback chain** — design in `docs/work/PLAN-STT-FALLBACK-CHAIN.md`. Deepgram primary → secondary cloud → local whisper.cpp.
3. **Full overlay wiring with native helpers** — the macOS Swift overlay and Windows C overlay are functional standalone; wiring them into the daemon's `NativeOverlayHandle` spawn path (replacing the stub in production) is a follow-up.
4. **System audio device selection UX** — currently uses default render endpoint; no UI for choosing a specific device.

## Review Checklist (for reviewer)

- [ ] Files match the scope described above
- [ ] No unrelated changes included
- [ ] Tests cover acceptance criteria from plan
- [ ] Code style matches CLAUDE.md rules
- [ ] No TODOs without linked task IDs
