# Round 232 - Listen STT Reservation Guards

Date: 2026-06-29 12:20 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner clicked Listen on/off repeatedly and saw balance move from about `$5.28` to `$4.68` within a few seconds.

## Root Cause

- Live STT uses a reserve-before-provider-dispatch model.
- A 10-minute Deepgram relay reservation for one source is about `28` cents.
- Listen starts both microphone and system audio, so one dual-source start can temporarily reserve about `56` cents.
- The macOS overlay treated `Starting` as `recordingActive = false`, so rapid clicks could emit more start events before the daemon had confirmed the first start.
- The daemon `start_audio_capture` always stopped/restarted instead of treating duplicate starts as idempotent.
- The relay source reserved a cloud STT session before the native helper had produced any audio bytes.
- The server rounded every opened relay settlement up to at least one second, even when no audio bytes were forwarded.
- The daemon refreshed balance on manual Stop before relay source settlement necessarily finished, so the UI could keep showing the reserved balance instead of the settled/refunded balance.

## Fix

- Added daemon audio-start state:
  - `starting`
  - `start_generation`
  - active `session_id`
- Duplicate starts while audio is starting or active are now ignored and logged instead of creating another capture runtime.
- Stop now invalidates a pending start generation, so a start that is canceled while still resolving devices cannot become active afterward.
- Live relay sources now start the native helper first and wait for the first PCM bytes before creating the server STT session.
- If the user stops before audio bytes arrive, no cloud STT session is reserved.
- Added an authenticated `/stt/session/cancel` endpoint and cloud-client method so the daemon can release a just-created reservation if the relay websocket fails before audio can stream.
- Server relay settlement now tracks forwarded audio bytes/chunks.
- A relay session with zero forwarded audio bytes settles with `elapsed_ms = 0`, `billable_seconds = 0`, and a full reservation refund.
- Added safe billing/log diagnostics:
  - hashed account id only
  - source
  - audio byte/chunk counts
  - reserved cents
  - settled cents
  - refunded cents
  - close reason
- After relay source tasks finish, the daemon refreshes the overlay balance again.
- If a relay source settles late after the normal stop wait, the daemon waits in a background task and refreshes balance when that late settlement finishes.
- macOS Listen button now debounces rapid clicks and treats `Starting` / `Stopping` as real in-flight states.
- macOS pill Listen toggle now uses `Connecting` instead of jumping straight to `Listening`.
- Windows record button now has bounce protection and a short restart guard after Stop.

## Mac / Windows Parity

- Shared daemon and server fixes protect both macOS and Windows.
- macOS received a fuller in-flight state guard because the Swift overlay receives listening state changes and has both expanded and pill Listen entry points.
- Windows received native record-button click bounce/restart protection in `native/windows/cue-overlay/main.c`.

## Verification

Passed:

- `cargo fmt --all`
- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo check -p cue-daemon`
- `cargo check --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml db::stt_accounting::tests -- --nocapture`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo build -p cue-cli -p cue-daemon`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA daemon restarted with the patched debug CLI, daemon, and macOS overlay.
- daemon pid `54661`
- active meeting id `3cce79e2-6031-406a-a7bc-2d9eb3c83e96`
- overlay visible `true`
- overlay capture excluded `false` because this is visible QA mode
- transcript segments `0`
- context items `0`

## Remaining QA / Gates

- In visible QA, click Listen on/off rapidly and confirm:
  - button does not emit repeated starts while `Starting`
  - balance either does not move before audio bytes arrive, or returns after settlement
  - daemon logs show ignored duplicate starts rather than multiple active sessions
- Test a normal short Listen with real speech and confirm the balance settles to the small actual STT charge, not the 10-minute reservation.
- Before release/upload, return to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
