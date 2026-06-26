# Round 046 - Overlay Pass-through Hit-test Fix - 2026-06-16

## Context

After adding the explicit interactive/pass-through toggle, Bluey controls could become hard to click in pass-through mode. The root cause was too narrow: pass-through preserved only the mode toggle and text regions, so primary controls such as new session, history, full size, hide, close, Tone, Listen, Screen, and Answer could be treated as transparent to mouse events.

## Change

- Pass-through mode now preserves the same explicit Bluey control chrome used by normal hit testing.
- The pass-through timer now has a short sticky-interactive delay, so moving onto a button makes the window clickable immediately instead of racing the next timer tick.
- Empty workspace/background regions still pass clicks through to the host app.
- Visible text/feed/canvas/transcript regions remain interactive for scroll/copy behavior.
- The mode toast now says: "Empty space passes through. Bluey controls and text stay available."
- Programmatic window frame changes now resync the AppKit content view to the real window content size and repin fixed chrome. This prevents the header/top bar from drifting downward after full-size -> restore.
- Borderless overlay button dispatch now has a manual NSButton path for pass-through edge cases. Mouse-down/up on a visible enabled button is routed to the button even when AppKit hit testing is flaky around transparent/layer-backed regions.

## Intended UX Contract

- **Interactive on:** the whole Bluey panel is clickable and draggable.
- **Click-through on:** Bluey stays visible; all Bluey controls remain clickable; empty background space passes through to the app underneath after the pointer is clearly away from controls.
- Modal overlays and the session drawer remain fully interactive in both modes.

## Verification

Run:

```bash
swift build -c release --package-path native/macos/cue-overlay
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh
cargo fmt --all --check
git diff --check
```

Manual smoke:

```bash
BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on
```

Then toggle click-through and verify header buttons, Tone, Listen, Screen, Answer, composer, and close/eye/full-size remain clickable while empty workspace clicks pass through.

Visual smoke on uno:

- Pill -> expanded: 158x32 -> 820x520.
- Expanded -> full-size -> restored: 820x520 -> 1664x1021 -> 820x520.
- Restored screenshot: `/tmp/bluey-expand-restore-final.png` shows the header pinned to the overlay top edge.
- Button smoke screenshots:
  - `/tmp/bluey-qc-tone-open-final.png` - Tone modal opens from the bottom control.
  - `/tmp/bluey-qc-listen-on-final.png` - Listen toggles active and live-caption strip becomes green.
  - `/tmp/bluey-qc-close-modal-final.png` - close confirmation opens centered inside the overlay.
  - `/tmp/bluey-qc-after-clicks-final.png` - Escape leaves the overlay clean with chrome still aligned.

No GitHub Actions were used for this verification; all builds and visual checks ran on the local Mac.
