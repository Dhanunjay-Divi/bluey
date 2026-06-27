# Round 210 - Listen Transcript Consume On Send

## Trigger

The owner showed a live overlay session where Bluey sent the previous spoken transcript again on the next answer. The visible example was:

- first ask: `Build me LRU cache.`
- next ask: `BuildMeLRUCache. Can you tell me...`

Expected behavior: once transcript text has been sent as an answer context, it should not remain in the local listen buffer or get prepended into the next question.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause/Fix

- macOS already cleared exact transcript lines after send, but live STT can emit a late cumulative partial/final with different spacing, such as `BuildMeLRUCache`.
- Exact fingerprint matching did not catch compact-spacing variants, so the old phrase could be treated as fresh transcript.
- The bottom live-caption strip also kept showing the old text after send, which made it look like the transcript was still pending.

Fixes:

- macOS now trims already-consumed transcript prefixes from incoming partial/final transcript text before that text enters the next answer buffer.
- The transcript consumed matcher now also compares compact alphanumeric fingerprints, so `Build me LRU cache` and `BuildMeLRUCache` are treated as the same consumed phrase.
- After a successful send, macOS resets the local caption strip to `Listening for follow-up...` while Listen is still active, or `Live captions preview` when idle.
- The daemon now treats compact-spacing final transcript variants as duplicates for stored meeting transcript deduplication.
- The daemon partial-to-final dedup now handles compact-spacing final variants too.
- Visible QA status now uses the same capture-visible debug gate as the overlay launch path, so local visible mode reports `overlay_capture_excluded: false` when it is actually capture-visible.

## Windows Parity Check

- The shared daemon duplicate guard applies to Windows too.
- Windows overlay does not append local transcript text into the outgoing ask the same way macOS does, but it did retain local caption buffers after send.
- Added `clear_local_transcript_context()` and call it after Windows sends a question, while keeping the explicit Clear button behavior unchanged.
- This clears only the Windows overlay preview/local send buffer; it does not emit `transcript_clear_requested` after normal send, so the daemon can still answer from the meeting transcript context.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
cargo test -p cue-daemon duplicate_transcript_detection_skips_same_speaker_and_cross_source_echoes --lib
cargo test -p cue-daemon --test live_transcript_dedup
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
cargo build -p cue-cli
cargo build -p cue-daemon --bin bluey-daemon
BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh
target/debug/bluey status
```

Final visible-mode status check:

- `overlay_visible: true`
- `overlay_capture_excluded: false`
- `transcript_segments: 0` in a fresh visible QA session

## Current State

- Local visible/debug Bluey is running from the fresh debug CLI, daemon, and overlay builds.
- The running overlay is intentionally capture-visible for QA screenshots.
- Return to normal capture-excluded mode with:

```bash
target/debug/bluey off
bluey on
```

## Remaining QA/Gates

- Manually test Listen with the exact repro:
  - speak `Build me LRU cache`
  - send/answer
  - keep Listen active and ask a follow-up
  - confirm the next question does not start with `BuildMeLRUCache`
- Public binaries still need a release build and deploy if this should ship to download users.
