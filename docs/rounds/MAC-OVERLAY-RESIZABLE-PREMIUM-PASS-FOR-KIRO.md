# Mac Overlay Resizable Premium Pass

Date: 2026-05-23
Owner: Codex

## Why

The previous overlay pass fixed the header cropping regression, but the product flow still felt wrong in two visible ways:

- `bluey on` opened the browser automatically when no account was linked.
- The collapsed pill and expanded overlay looked too heavy, and the expanded panel could not be resized.

The intended alpha flow is: `bluey on` starts Bluey locally, shows the centered pill, and lets the user sign in only when they choose to.

## Changes

- `bluey on` no longer opens `https://bluey.sh/link` automatically.
- Unlinked users now see a short boot hint: `bluey login` or the link URL when they are ready.
- The collapsed macOS pill is smaller: `118x34`, tighter logo/title/dot spacing, softer border and shadow.
- The expanded macOS panel now creates a resizable borderless `NSWindow`.
- Expanded size constraints now allow bounded width and height resizing while keeping a minimum usable layout.
- Canvas open/close no longer permanently caps the window width to compact/canvas widths; users can keep their preferred size.
- The visual smoke script was updated to recognize the smaller pill and enforce the new resizable contract markers.

## UX Contract

- Launch state: centered pill only.
- Click pill: pill disappears, expanded overlay opens.
- Hide: expanded overlay collapses back to pill.
- Close/X: confirm turn-off and tell the user to run `bluey on` to start again.
- Expanded overlay: movable and resizable, with header/composer preserved on screen.
- Workspace pass-through remains limited to the non-interactive middle workspace; header, session drawer, transcript strip, attachment row, and composer stay clickable.

## Verification

Run:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cd server && cargo test
cd crates/cue-dashboard/ui && npm run build
swift build -c release --package-path native/macos/cue-overlay
scripts/macos-overlay-visual-smoke.sh
git diff --check
```

Manual smoke to do on the Mac desktop:

1. Run `bluey off`, then `bluey on`.
2. Confirm no browser opens automatically.
3. Confirm the small centered pill is visible.
4. Click the pill and confirm the expanded panel opens.
5. Resize the expanded panel from the edges/corners and verify the header, balance/model controls, transcript strip, and composer remain visible.
6. Click Hide and confirm the pill returns.

