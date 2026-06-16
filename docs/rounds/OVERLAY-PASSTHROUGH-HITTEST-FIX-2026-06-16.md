# Overlay Pass-through Hit-test Fix - 2026-06-16

## Context

After adding the explicit interactive/pass-through toggle, Bluey controls could become hard to click in pass-through mode. The root cause was too narrow: pass-through preserved only the mode toggle and text regions, so primary controls such as new session, history, full size, hide, close, Tone, Listen, Screen, and Answer could be treated as transparent to mouse events.

## Change

- Pass-through mode now preserves the same explicit Bluey control chrome used by normal hit testing.
- Empty workspace/background regions still pass clicks through to the host app.
- Visible text/feed/canvas/transcript regions remain interactive for scroll/copy behavior.
- The mode toast now says: "Empty space passes through. Bluey controls and text stay available."

## Intended UX Contract

- **Interactive on:** the whole Bluey panel is clickable and draggable.
- **Click-through on:** Bluey stays visible; all Bluey controls remain clickable; empty background space passes through to the app underneath.
- Modal overlays and the session drawer remain fully interactive in both modes.

## Verification

Run:

```bash
swift build -c release --package-path native/macos/cue-overlay
cargo fmt --all --check
git diff --check
```

Manual smoke:

```bash
BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on
```

Then toggle click-through and verify header buttons, Tone, Listen, Screen, Answer, composer, and close/eye/full-size remain clickable while empty workspace clicks pass through.
