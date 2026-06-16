# Overlay Feed Header Clip Fix - 2026-06-16

## Problem

During visible-overlay QA, right-side transcript/question bubbles could appear to slide above or under the fixed Bluey header bar. The content was not escaping into another window; the feed scroll surface was allowed to sit behind translucent fixed chrome, and the feed auto-scroll path assumed the document view used the opposite vertical coordinate orientation.

## Fix

- `FeedView.scrollToBottom()` now respects the stack view's coordinate orientation before choosing the scroll target.
- The feed scroll view has small top/bottom content insets so clipped rows do not visually collide with the chrome.
- The expanded overlay header is now fully opaque, so scrolled content cannot show through the header.
- `keepFixedChromeInBounds()` now clamps the child feed/canvas frames after clamping the outer workspace frame, preventing AppKit fallback layout from leaving the feed behind the header.

## Verification

Commands run:

```bash
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

Manual visible QA:

```bash
BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on
./target/debug/bluey overlay show
```

Then synthetic question/answer cards were pushed through daemon IPC and the screen was captured:

```text
/tmp/bluey-debug/header-feed-clip-fixed-2.png
```

Result: right-side user/transcript bubbles and left-side Bluey answers stay below the fixed header. The header no longer shows scrolled card text through its background.

`BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` remains a local QA-only flag and must not be shipped in release artifacts.
