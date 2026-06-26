# Round 051 - Overlay Composer Density Pass - 2026-06-17

## Goal

Make the expanded Bluey overlay bottom chrome feel lighter and leave more vertical space for questions, transcripts, and answers.

## Changes

- Reduced the default composer bar height from 94 px to 78 px.
- Reduced the live captions strip height from 26 px to 22 px.
- Reduced the composer input height from 42 px to 34 px.
- Tightened bottom-row controls: Listen, Answer, Tone, Opacity, Auto, Screen, and attach button.
- Reduced button corner radii and font sizes slightly so controls read as compact chrome instead of large cards.
- Lowered multiline composer growth cap from 88 px to 68 px so long typed questions do not eat the answer area.
- Moved the manual fixed-chrome geometry math to the same constants used by Auto Layout, so resize/fullscreen/restore uses the compact values consistently.

## Verification

```bash
swift build -c release --package-path native/macos/cue-overlay
```

Result: pass.

## Visual QA Focus

- Expanded overlay bottom chrome should be visibly shorter than the previous screenshot.
- The captions strip should still show live mic/system status without wrapping.
- Listen and Answer should remain readable and clickable.
- Opacity slider should remain usable.
- Multiline typed questions should grow modestly and preserve the chat/feed area.
