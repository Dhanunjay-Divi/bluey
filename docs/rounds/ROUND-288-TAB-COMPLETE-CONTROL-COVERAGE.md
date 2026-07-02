# Round 288 - Tab Complete Control Coverage

## Trigger

The owner asked whether `Tab` selects everything in the overlay: Ask text box, Tone, Opacity, Auto-send, Auto/model menu, Screen, Listen, and Answer.

## Root Cause

Rounds 286 and 287 added keyboard navigation and made the macOS focus ring visible, but the navigation model was still button-only:

- macOS tracked `keyboardFocusedButton: NSButton?`, so the Ask field, opacity control, and popup menus were skipped.
- Windows compact overlay had `Tab` support for buttons, but skipped the Ask edit field and Auto-send combo box.

## Fix

- Replaced macOS button-only focus tracking with control-level focus tracking.
- Added macOS Tab targets for:
  - header buttons
  - history/session drawer controls when visible
  - live transcript clear
  - Ask input surface
  - attach
  - Tone
  - Opacity
  - Auto-send menu
  - Auto/Instant/Balanced/Deep model menu
  - Listen
  - Answer
  - Screen
- `Enter` / `Space` now activates the selected macOS control:
  - Ask input focuses the composer
  - menus open
  - buttons click
  - opacity focuses the opacity control
- Arrow keys adjust opacity while opacity is selected.
- Tightened modal/drawer recursive control discovery so static labels do not become fake Tab stops.
- Updated shortcut guide copy from "buttons" to "controls."
- Windows parity:
  - `Tab` order now includes Ask edit field and Auto-send combo box.
  - `Enter` / `Space` opens the Auto-send combo when selected.
  - Windows shortcut guide copy now says controls instead of buttons.
- Bumped desktop workspace version to `0.1.42`.

## Expected Behavior

- When click-through is off, `Tab` and `Shift+Tab` cycle through visible enabled controls.
- The selected macOS control gets the blue focus ring.
- `Enter` / `Space` activates the selected control.
- When Ask is focused, normal text editing wins: typing, arrows, delete, select-all, and Enter behave like the Ask field.
- When click-through is on, use global shortcuts or the visible move handle; blank overlay space continues to click behind Bluey.

## Verification

Passed locally:

```bash
swift build -c debug --package-path native/macos/cue-overlay
cargo check -p cue-daemon --quiet
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/latest.json` reports version `0.1.42`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.42/bluey-0.1.42-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `44918f0d5adbcf8b92ad8541a96dcc1e7d3e374bfc22d7e650b385786f63f82f`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.42` is live and downloadable from the droplet.
- Windows source parity is implemented and syntax-checked.
- The public release manifest still advertises `darwin-arm64` only until a Windows artifact is packaged and published.

## Remaining QA

- Owner should run `bluey off && bluey on`, confirm auto-update to `0.1.42`, then test:
  - `Tab` reaches Ask, Tone, Opacity, Auto-send, Auto/model, Screen, Listen, and Answer.
  - `Shift+Tab` moves backward.
  - `Enter` / `Space` activates each selected control.
  - Ask field keeps normal typing/editing behavior.
