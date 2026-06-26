# Round 042 - Overlay Transcript Chat Layout - 2026-06-15

## Goal

Make the expanded Bluey overlay behave like a stable chat surface:

- transcript/user speech appears on the right
- Bluey answers appear on the left
- source labels collapse into `Mic` / `System` only when the source changes
- the feed scrolls inside a fixed board instead of pushing under the header or composer
- pressing Answer with an empty composer sends the latest transcript context, not a generic placeholder

## Implementation

Changed `native/macos/cue-overlay/Sources/cue-overlay/main.swift`.

Key changes:

- Transcript cards are now rendered into the feed as right-aligned chat bubbles.
- Consecutive transcript chunks from the same source merge into one bubble to avoid repeated `System:` / `Mic:` noise.
- Transcript text strips repeated source prefixes and ignores dev-audio mock chunks.
- Bluey answer cards remain left-aligned and keep the copy action.
- Right-side user/transcript bubbles do not show a copy action.
- The feed and workspace now mask overflow so cards cannot visually slide behind fixed chrome.
- Scroll wheel forwarding was added for feed/canvas/session drawer regions.
- `Answer` with an empty composer now uses the latest transcript snippets; if no transcript exists, Bluey shows a small system toast.
- The live strip uses concise text such as `Transcribing - Mic - ...` or `Captured - System - ...`.

## Verification

Ran:

```bash
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh
swift build -c release --package-path native/macos/cue-overlay
git diff --check
git diff --cached --check
```

Visual QA used local-only debug visibility:

```bash
BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on
./target/debug/bluey overlay show
```

Then injected synthetic transcript/answer cards through the daemon IPC. The captured result showed:

- right-aligned grouped transcript bubbles
- left-aligned Bluey answer
- fixed header/composer
- clipped scrollable feed
- live strip using `Captured - Mic/System`

Screenshot artifact:

```text
/tmp/bluey-debug/final-transcript-layout.png
```

The debug-visible overlay was stopped after verification.

## Notes

`BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` remains a local QA-only flag and must not be shipped or documented as a customer path.

`bluey-dev.db` was left untracked and untouched.
