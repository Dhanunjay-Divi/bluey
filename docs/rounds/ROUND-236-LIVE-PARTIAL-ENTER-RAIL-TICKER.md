# Round 236 - Live Partial Enter And Rail Ticker

Date: 2026-06-29 19:03 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner pressed Enter while Listen was showing a live caption and saw Bluey
send a generic Question card:

`Answer the latest live captions from the current session transcript. Treat the transcript as the user's current question or working context.`

The answer then failed with the user-facing provider-error fallback. The owner
also pointed out that the live transcript rail behaves more like a static
caption than a horizontal live scroll/ticker.

## Root Cause

The daemon status at the time showed `transcript_segments: 0` even though the
overlay rail showed a partial live caption such as `Mic: Sure.`. That means the
caption existed only inside the overlay's live partial/preview memory and had
not yet become a finalized daemon transcript segment.

Round 234 made short finalized/live transcript asks visible, but macOS could
still fall back to the generic live-caption prompt when only the preview rail
had usable text. For longer live captions, the old macOS and Windows behavior
could also send the generic instruction instead of the caption text itself.
That is risky because preview-only captions may not exist anywhere else for the
backend to read.

The transcript rail also resized from a simple font-width estimate and only
scrolled asynchronously. After relayout or long attributed text, it could remain
clipped at the beginning instead of following the newest caption tail.

## Fix

macOS overlay:

- Added a live-preview fallback for answer composition:
  - latest live transcript line
  - latest live line by source
  - merged preview bodies by source
  - current rail text as last resort
- Bounded transcript text sent from the overlay to
  `ChromeMetrics.transcriptPreviewMemoryChars` so live-caption sends do not
  overload the UI or model payload.
- Manual Enter/Answer now sends actual caption text when available instead of
  the generic live-caption instruction.
- Auto-send-after-stop now sends actual bounded caption text for longer
  captions instead of a generic prompt.
- Duplicate suppression and "has transcript context" checks now include
  preview-only caption memory.
- `ask_answer_sent` / duplicate-suppressed lifecycle logs now include
  `preview_transcript_context` so we can diagnose partial-caption sends without
  logging personal transcript text.
- Current session emptiness now treats preview-only caption memory as real
  session state.
- The transcript rail now:
  - follows the newest text immediately and again on the next run loop
  - re-follows the tail after layout/resizing
  - measures attributed text width instead of raw font-only width
  - allows horizontal elasticity and disables predominant-axis filtering

Windows overlay:

- Added parity for longer live transcript sends:
  - if the short visible transcript question is unavailable, Windows now sends
    the bounded live transcript text instead of the generic live-caption prompt.

## Verification

Passed:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
native/macos/cue-overlay/build.sh
BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh
/Users/uno/Downloads/cue/target/debug/bluey status
git diff --check
```

Visible QA restart result:

- `overlay_visible: true`
- `overlay_capture_excluded: false`
- daemon pid: `33583`
- active meeting id: `ea11ad65-014f-419c-adbd-0b4a4ed64e7b`
- `transcript_segments: 0` at startup, which is expected before final captions
  arrive and is exactly why the overlay-side partial fallback matters.

## Current State

The local debug Bluey was restarted in visible overlay QA mode and is using the
patched overlay source.

Return to normal capture-excluded mode before release/upload:

```bash
/Users/uno/Downloads/cue/target/debug/bluey off
/Users/uno/Downloads/cue/target/debug/bluey on
/Users/uno/Downloads/cue/target/debug/bluey status
```

Normal/release status should show `overlay_capture_excluded: true`.

## Remaining QA And Gates

- Speak a short live caption such as "Explain LRU cache", press Enter while it
  is still partial, and verify the Question card uses the caption text, not the
  generic live-caption instruction.
- Speak a longer live caption and verify Bluey sends a bounded transcript tail
  while the bottom rail follows the newest words horizontally.
- Verify duplicate suppression after pressing Enter twice quickly from the same
  live caption.
- The daemon log is still showing many cloud object-sync failures:
  - `object sync is not configured`
  - `503 Service Unavailable`
  - `cloud object upload failed; continuing with text sync`
  - occasional `sync failed` / `500 Internal Server Error`
- Those sync errors are separate from this overlay fix and should be handled in
  a backend/cloud sync round so provider/sync failures are surfaced clearly
  without confusing them with Listen transcript behavior.
