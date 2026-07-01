# Round 278 - Click-Through Default Move Handle

## Trigger

The overlay still felt backwards for normal use:

- Bluey opened with click-through enabled by default.
- Blank space was expected to drag Bluey when click-through was off, including history and keyboard popups.
- When click-through was on, blank space should pass through, but there still needed to be one clear way to reposition the overlay.
- The keyboard shortcut popup felt too tall/cramped and visually close to being cut off.

## Root Cause

- macOS defaulted `passThroughMode` to `true`, so first launch favored pass-through instead of normal interactive use.
- The pass-through drag affordance reused the Bluey logo/name area, which made the movement target implicit and too easy to confuse with blank-space click-through.
- History and modal overlays consumed clicks but did not route blank/panel mouse-downs into the window drag path.
- Windows defaulted to click-through-on equivalent behavior and also used the brand area as a hidden move handle.

## Fix

- macOS now defaults click-through off:
  - blank Bluey space receives mouse events
  - blank Bluey space can drag the window
  - history and keyboard/tone modal blank areas also drag the window
  - real controls and editable/selectable text keep their normal clicks
- macOS click-through-on mode now shows a dedicated blue four-direction move handle in the header.
  - blank space clicks the app behind Bluey
  - visible controls remain clickable
  - only the blue move handle drags the overlay
- The old logo/name move target is no longer used in click-through-on mode.
- macOS shortcut help is more compact:
  - click-through-off copy explains drag-anywhere behavior
  - click-through-on copy explains global shortcuts plus the blue move handle
  - inside-Bluey shortcuts are grouped in paired rows
- Windows parity:
  - default mode is click-through off
  - whole-window drag remains available when click-through is off
  - click-through-on mode has a visible cyan move handle
  - Windows hit testing returns `HTTRANSPARENT` for blank click-through space and `HTCAPTION` only for the move handle
  - Windows help/shortcut copy mirrors the new mental model
- Desktop workspace version bumped to `0.1.32`.

## Verification

Passed:

- `swift build -c debug --package-path native/macos/cue-overlay`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo check -p cue-daemon --offline`
- `cargo check -p cue-dashboard --locked`
- `cargo test -p cue-daemon --lib --locked`
- `cargo test -p cue-core sign_in_event_serializes --locked`
- `git diff --check`

- Release artifact dev-flag/secret scan passed.
- Published `v0.1.32` to `https://bluey.sh/latest.json`.
- Live artifact:
  `https://bluey.sh/releases/v0.1.32/bluey-0.1.32-darwin-arm64.tar.gz`
- Live SHA256:
  `9883b0bff4be1e8967b463295011bd631fc0d7d6d43f3c86c32ebb3475e6de4f`
- `latest.json.sig` verified successfully.
- `https://bluey.sh/install.sh` serves `application/x-shellscript`.
- Local install updated to `bluey 0.1.32` and restarted.
- Local status showed `overlay_capture_excluded: true`.

## Current State

The intended interaction model is now:

- Default: click-through off, drag blank Bluey space anywhere.
- Popups open: blank popup/panel area can still drag Bluey.
- Click-through on: blank space passes through; drag only the blue/cyan move handle.

## Remaining QA

- Run a live visual smoke in normal capture-excluded mode after release.
- Build Windows artifact on the Windows/MSVC runner; this Mac can only syntax-check the Windows source.
