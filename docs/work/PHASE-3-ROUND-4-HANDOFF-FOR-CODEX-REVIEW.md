# Phase 3 Round 4 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-4`
**Base**: main tip (post-Round-3-merge)
**Authors**: kiro (subagents 1 & 2), uno (user — Windows overlay + MSVC fixes)

## Scope

Round 4 of Phase 3. Fixes the overlay restart-loop race, adds the full restart-on-crash supervisor, ships system audio capture via native OS helpers (macOS ScreenCaptureKit + Windows WASAPI), and adds a Windows overlay rendered with Direct2D.

### Commits (4 ahead of main)

```
6fc8682 feat(daemon): system audio capture via native helpers (macOS ScreenCaptureKit + Windows WASAPI) [P3.R4 stage 2]
329f324 fix(windows): make native helper builds pass MSVC
6f21864 feat(windows): render overlay with Direct2D and add Windows CI
19ff43a fix(daemon): restructure overlay supervisor to own send_rx, fixes restart-loop race [P3.R4 stage 1]
```

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 136 pass
cd crates/cue-dashboard/ui && npm run build          ✅ pass
swift build (native/macos/cue-audio)                 ✅ pass
git -P diff --check main..HEAD                       ✅ clean
```

### Test count delta

| Tier | Round 3 (main) | Round 4 | Δ |
|------|----------------|---------|---|
| cue-core lib | 45 | 45 | — |
| cue-daemon lib | 78 | 81 | +3 (2 system_capture unit + 1 async unit) |
| Integration: pipeline | 4 | 4 | — |
| Integration: overlay pipe | 3 | 3 | — |
| Integration: overlay restart | 0 | 1 | +1 |
| Integration: system audio | 0 | 2 | +2 |
| Ignored (hardware) | 1 | 1 | — |
| **Total running** | **130** | **136** | **+6** |

## Architecture Diagram — Round 4 Additions

```
                    ┌──────────────────────────────────────────────────────┐
                    │                    cue-daemon                        │
                    │                                                      │
Mic ──▶ Framer ──▶ TwoStageVad ──▶ SttProvider (Deepgram)                 │
                                                                          │
                    ┌─────────── NEW: System Audio Path ──────────────┐   │
                    │                                                  │   │
                    │  BLUEY_SYSTEM_AUDIO_CONTINUOUS=1                  │   │
                    │         │                                         │   │
                    │         ▼                                         │   │
                    │  SystemAudioCapture::start(sender)                │   │
                    │         │                                         │   │
                    │         │ spawns native helper child              │   │
                    │         │ (--source system --continuous)          │   │
                    │         ▼                                         │   │
                    │  ┌──────────────────────────┐                    │   │
                    │  │ macOS: bluey-audio-macos  │                    │   │
                    │  │   ScreenCaptureKit        │                    │   │
                    │  │   48kHz float → 16kHz i16 │                    │   │
                    │  └──────────┬───────────────┘                    │   │
                    │             │ stdout: raw i16 LE PCM              │   │
                    │  ┌──────────┴───────────────┐                    │   │
                    │  │ Windows: bluey-audio.exe  │                    │   │
                    │  │   WASAPI loopback         │                    │   │
                    │  │   mix fmt → 16kHz i16     │                    │   │
                    │  └──────────┬───────────────┘                    │   │
                    │             │                                     │   │
                    │             ▼                                     │   │
                    │  read_child_stdout()                              │   │
                    │  frame into 20ms AudioChunks (320 samples)       │   │
                    │             │                                     │   │
                    │             ▼                                     │   │
                    │  mpsc::UnboundedSender<AudioChunk>                │   │
                    │             │                                     │   │
                    │             ▼                                     │   │
                    │  (eventually → STT pipeline, not yet wired)       │   │
                    │                                                   │   │
                    │  supervisor_loop: restart on crash                │   │
                    │  (exponential backoff 250ms → 5s, max 5 attempts) │   │
                    └───────────────────────────────────────────────────┘   │
                                                                           │
                    ┌─────────── Overlay Supervisor (restructured) ─────┐  │
                    │                                                    │  │
                    │  run_supervisor() owns send_rx                     │  │
                    │       │                                            │  │
                    │       ▼                                            │  │
                    │  run_one_child(child, send_rx, recv_tx, carryover) │  │
                    │       │                                            │  │
                    │       │ tokio::select! {                           │  │
                    │       │   child.wait() => exit observed            │  │
                    │       │   send_rx.recv() => write to stdin         │  │
                    │       │ }                                          │  │
                    │       │                                            │  │
                    │       │ returns (send_rx, carryover, clean_exit)   │  │
                    │       │                                            │  │
                    │       ▼                                            │  │
                    │  if !clean_exit && attempts < MAX:                 │  │
                    │    sleep(restart_delay) → spawn_child → loop       │  │
                    │                                                    │  │
                    └────────────────────────────────────────────────────┘  │
                    └──────────────────────────────────────────────────────┘
```

## Per-Commit Review Checklist

### `19ff43a` — Overlay supervisor restructure (restart-loop fix)

- [ ] `run_one_child` takes ownership of `send_rx` and returns it on exit — no channel leak
- [ ] `tokio::select!` is biased: `child.wait()` checked first (prevents writing to dead stdin)
- [ ] `msgs_written` / `msgs_acked` tracking: written increments on successful `write_msg`, acked increments when reader sees a line from child
- [ ] Carryover semantics: if `written > acked` on non-clean exit, `last_msg` becomes carryover for next generation
- [ ] Carryover is written FIRST to the new child's stdin before entering the select loop
- [ ] `MAX_RESTART_ATTEMPTS = 5` — supervisor gives up and sets `Failed` state after exceeding
- [ ] `restart_delay` exponential backoff: 250ms → 500ms → 1s → 2s → 4s → 5s (cap)
- [ ] On spawn failure during restart, retries once more before giving up
- [ ] `shutdown()` sets `shutdown_requested` atomic → supervisor observes and returns cleanly
- [ ] `overlay-stub-oneshot` binary: reads one msg, sends Pong, exits with code 7 (non-zero = crash)
- [ ] Integration test: first Pong received → child crashes → supervisor restarts → second Pong received
- [ ] Integration test uses `CARGO_BIN_EXE_overlay-stub-oneshot` (cross-platform stable path)
- [ ] No `unwrap()` in non-test code except `.expect("stdin piped")` / `.expect("stdout piped")` (guaranteed by `Stdio::piped()`)

### `6f21864` — Windows overlay with Direct2D

- [ ] `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST` — overlay stays on top, excluded from taskbar
- [ ] `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` — overlay excluded from screen capture
- [ ] Direct2D rendering path (`paint_with_d2d`) with GDI fallback if D2D init fails
- [ ] NDJSON IPC on stdin (read in `stdin_thread`) / stdout (emit events)
- [ ] `emit_ready()` on startup — daemon knows overlay is alive
- [ ] Collapsed pill mode with drag support
- [ ] Proper COM cleanup in `WM_DESTROY` and `release_d2d_resources`
- [ ] No memory leaks: all `CreateSolidBrush` / `CreatePen` / `CreateFont` paired with `DeleteObject`
- [ ] Windows CI added to `.github/workflows/ci.yml`

### `329f324` — MSVC build fixes

- [ ] `DEFINE_GUID` macros guarded by `#ifdef _MSC_VER` (MinGW provides them differently)
- [ ] `#include <initguid.h>` before COM headers
- [ ] Build scripts updated for MSVC toolchain

### `6fc8682` — System audio capture via native helpers

- [ ] `SystemAudioCapture::start(sender)` spawns the platform binary with `--source system --continuous`
- [ ] `BLUEY_SYSTEM_AUDIO_BINARY` env var override for testing (points to stub)
- [ ] `resolve_binary()` checks env override first, then platform-specific path
- [ ] `read_child_stdout` frames raw bytes into exactly 320-sample (640-byte) chunks
- [ ] Each chunk has `source: AudioSource::System`, `sample_rate: SampleRate::SR_16K`
- [ ] `epoch_ms()` timestamp on each chunk (monotonic enough for ordering)
- [ ] `supervisor_loop` restarts on non-zero exit, gives up after 5 attempts
- [ ] `SystemAudioCapture::stop()` sets atomic flag → child killed → task joins within 3s timeout
- [ ] `Drop` impl sets stop flag (safety net if `stop()` not called)
- [ ] macOS Swift helper: `SCStream` with `capturesAudio=true`, `excludesCurrentProcessAudio=true`
- [ ] macOS helper: 48kHz → 16kHz resampling via carry-based decimator in `PCM16Writer`
- [ ] macOS helper: `--continuous` mode runs forever; non-continuous exits after `--duration-ms`
- [ ] macOS helper: requires macOS 13+ (guarded by `@available(macOS 13.0, *)`)
- [ ] Windows C helper: `AUDCLNT_STREAMFLAGS_LOOPBACK` on default render endpoint
- [ ] Windows C helper: handles float32, int16, int24, int32 PCM via `read_channel_sample`
- [ ] Windows C helper: `_setmode(_fileno(stdout), _O_BINARY)` — no CRLF corruption
- [ ] Windows C helper: `fflush(stdout)` after each packet (no buffering delay)
- [ ] `system-audio-stub` binary: outputs 2s of 440Hz sine as 16kHz mono i16 LE
- [ ] Integration test `system_audio_capture_receives_chunks_from_stub`: receives ≥5 chunks, validates source/rate/length
- [ ] Integration test `system_audio_capture_stops_cleanly`: stop doesn't hang
- [ ] `app.rs` wiring: `BLUEY_SYSTEM_AUDIO_CONTINUOUS=1` gates the feature; chunks forwarded to daemon channel

### Security

- [ ] No secrets in any new code
- [ ] System audio capture does not log audio content — only metadata (chunk count, errors)
- [ ] Overlay `WDA_EXCLUDEFROMCAPTURE` prevents sensitive overlay content from appearing in screen recordings

### Style / Hygiene

- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] No new `unwrap()` in non-test code beyond documented `.expect()` calls
- [ ] Module-level doc comments on all new files

## Explicit Deferrals (NOT in Round 4)

1. **STT routing for system audio** — chunks reach the daemon channel but are not forwarded to Deepgram or any STT provider. Requires a source-aware multiplexer.
2. **STT fallback chain** — design in `docs/work/PLAN-STT-FALLBACK-CHAIN.md`.
3. **Real overlay spawn path** — daemon still uses `overlay-stub` in tests; production spawn of the Swift/C overlay binaries is a follow-up.
4. **System audio device selection** — uses OS default; no UI picker.

## Verdict Request

Codex: review the 4 commits + new modules + integration tests + IMPL doc. Write `docs/work/REVIEW-PHASE-3-ROUND-4.md` with verdict.

- 🟢 **ACCEPT** → merge to main, start Round 5 (STT routing for system audio + STT fallback chain)
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 5
- 🔴 **REQUEST CHANGES** → kiro writes `docs/work/FIX-PHASE-3-ROUND-4.md` and re-hands
