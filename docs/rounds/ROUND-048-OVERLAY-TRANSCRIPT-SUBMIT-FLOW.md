# Round 048 - Overlay Transcript Submit Flow - 2026-06-16

## Goal

Make live transcript feel like input context, not chat spam:

- while Bluey is listening, mic/system transcript stays in the lower horizontal ticker
- when the user presses Answer, Enter, or the configured shortcut, the submitted transcript/context appears once as the right-side user bubble
- Bluey answers stay on the left
- copy affordances appear only for code/system-design artifact output, where copy is genuinely useful

## Changes

- `FeedView.push(_:)` now treats incoming `transcript` cards as ticker/memory updates only. It still emits `card_rendered`, but it does not render a chat bubble.
- `ExpandedPanelView.appendLiveTranscript(...)` still remembers partial/final transcript for Answer, but final chunks no longer push transcript cards into the feed.
- `FeedView` now shows the copy button only for left-side cards with a `code` or `system_design` artifact. Normal prose answers no longer get a repeated copy icon.

## User Flow

1. User clicks Listen.
2. Current mic/system text appears in the live captions strip.
3. User presses Answer/Enter.
4. Bluey submits the latest typed text plus remembered transcript context.
5. The submitted prompt appears once on the right side of chat.
6. The streamed answer appears on the left.
7. If the answer opens a code/system-design canvas, copy remains available on the artifact output.

## Verification

Commands run:

```bash
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

Visual QA ran with the local capture-visible escape hatch:

```bash
BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on
./target/debug/bluey overlay show
```

Screenshots inspected:

- `/tmp/bluey-debug/transcript-ticker-only.png` - transcript cards remained only in the lower ticker
- `/tmp/bluey-debug/transcript-submit-copy-scope.png` - submitted transcript appeared once as the user bubble; normal answer had no copy button; code artifact still opened canvas/copy affordance

Important: `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` is for local visual QA only and must not ship in production release artifacts.

## Notes For Review

- The old explicit `pushTranscript(...)` helper is still present for future deliberate transcript rendering, but live transcript capture and daemon `transcript` push cards no longer use it.
- The daemon already emits the submitted user/question card during answer handling, so this patch does not create a separate local duplicate.
