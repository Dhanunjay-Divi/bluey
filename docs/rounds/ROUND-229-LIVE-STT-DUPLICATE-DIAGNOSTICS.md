# Round 229 - Live STT Duplicate Diagnostics

Date: 2026-06-28 03:02 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported that live transcript accuracy still felt bad and asked why the transcript was not coming accurately.

## Findings

- The active visible QA session initially reported `transcript_segments: 0`, so any current "no transcript" case is before answer generation: audio capture, live STT relay, provider events, or overlay display.
- Local settings have both `audio_system_enabled: true` and `audio_microphone_enabled: true`. This is useful for meetings, but it can capture the same speech twice when the microphone also hears speaker audio.
- Backend duplicate detection only caught exact normalized text or compact exact text. Near-identical mic/system echoes could still be stored and later sent as answer context.
- Backend final transcript handling sent both:
  - `TranscriptFinal` for the live caption strip
  - a second `PushCard` transcript card with the same final text
- macOS consumes both paths, so a single final STT result could appear as duplicated or stitched transcript UI. Windows mostly uses the final event for the preview strip, so the visible duplication was more likely on macOS.
- Existing privacy-safe logs showed stored mic final segments and clear events, but did not have enough live relay source/chunk/event metadata to debug "no transcript" or "bad transcript" reports without reproducing user audio.

## Fix

- Final transcript display now uses the `TranscriptFinal` event path only; the daemon no longer pushes an additional transcript feed card for the same final text.
- `add_audio_transcript_segment` now returns whether a final segment was actually stored, so duplicate/skipped finals do not inflate emitted transcript metrics.
- Added fuzzy near-duplicate transcript detection:
  - keeps exact normalized and compact duplicate detection
  - catches high-overlap mic/system echo text
  - avoids treating real longer follow-up continuations as duplicates
- Added privacy-safe live STT relay diagnostics:
  - relay source started with source, stream id, provider label, model, and helper arg
  - periodic audio chunk forwarding metadata with source, sequence, bytes, and duration
  - transcript event shape with source, sequence, final/partial flag, character count, and word count
  - no raw transcript text is logged by the new diagnostics

## Mac / Windows Parity

- The core duplicate detection, transcript storage, and relay diagnostics are shared daemon behavior for macOS and Windows.
- No native overlay behavior changed in this round.
- macOS and Windows overlay syntax checks passed.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-daemon duplicate_transcript_detection --lib`
- `cargo test -p cue-daemon --lib` (`277 passed`, `2 ignored`)
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `git diff --check`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA overlay was restarted from the rebuilt debug daemon:
  - daemon pid `77359`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`
  - transcript segments `0` until Listen receives final STT

## Remaining QA / Gates

- Press Listen and speak a short phrase once.
- Confirm:
  - the live strip updates once
  - the same final transcript is not also inserted as a duplicate feed card
  - `bluey status` increments `transcript_segments` after a final STT event
  - logs show relay source/chunk/event metadata without raw text
- Test with both system and mic enabled, then with only mic enabled, to confirm whether speaker echo is the remaining accuracy problem.
- Before release/upload, return visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
