# Round 272 - Keyboard Editing Shortcut Scope

## Trigger

The owner reported that `Ctrl+Option+H` closed or hid Bluey, arrow keys did not feel like normal text editing, `Cmd+A` followed by Delete did not clear the Ask input, and asked whether Enter should submit while interactive mode is off.

## Root Cause

macOS still had an event-monitor fallback that passed any `Ctrl+Option+<key>` through the old inside-Bluey shortcut router. That let `Ctrl+Option+H` behave like the optional local `H` History shortcut instead of being ignored.

The Ask composer routing also refocused and re-armed the typing caret before handling Delete. When the user selected all text with `Cmd+A`, the following Delete could collapse the selection before the delete reached the text view.

## Fix

- macOS global shortcut routing now consumes only the official global shortcuts:
  - `Ctrl+Option+B` hide/restore
  - `Ctrl+Option+T` text input
  - `Ctrl+Option+L` Listen
  - `Ctrl+Option+S` Screen
  - `Ctrl+Option+I` Interactive/click-through
  - `Ctrl+Option+Enter` Answer
- `Ctrl+Option+H` and other unassigned modified keys now return `false` and pass through to macOS instead of opening/closing Bluey UI.
- Local `Ctrl+<key>` shortcuts no longer trigger the old optional inside-Bluey shortcuts.
- Ask text editing no longer refocuses/re-arms the composer when it is already the first responder.
- Delete, Return, Home/End, Page Up/Down, and arrow keys are forwarded to the composer text view so normal text editing behavior is preserved.
- Enter still submits while Ask is focused. Shift+Enter remains the multiline path. From outside Ask, users should use `Ctrl+Option+Enter`.

## Windows Parity

Windows already registers only the official global keys (`Ctrl+Alt+B/T/L/S/I/Enter`) and does not register `Ctrl+Alt+H`. The Windows Ask edit control already requests arrow-key handling with `DLGC_WANTARROWS` and keeps selected text visible with `ES_NOHIDESEL`.

No Windows product code change was needed in this round, but the Windows overlay source was syntax-checked.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only -DUNICODE -D_UNICODE native/windows/cue-overlay/main.c`
- `git diff --check`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=release bash native/macos/cue-overlay/build.sh`
- Local hot install, ad-hoc sign, and restart.
- Live shortcut smoke before release:
  - `Ctrl+Option+H` left `overlay_visible: true`
  - `Ctrl+Option+B` changed `overlay_visible: false`
  - `Ctrl+Option+B` again changed `overlay_visible: true`
- Bumped desktop workspace to `0.1.26`.
- `BLUEY_UPDATE_PUBKEY=... make package-darwin-arm64`
- `scripts/publish-bluey-release.sh` dev-flag/secret scan passed.
- Live `https://bluey.sh/latest.json` reports `0.1.26`.
- Live `latest.json.sig` is present.
- Live `SHA256SUMS.txt` matches the local artifact checksum.
- Local `bluey update` updated from `0.1.25` to `0.1.26`.
- Post-update smoke:
  - `Ctrl+Option+H` left `overlay_visible: true`
  - `Ctrl+Option+B` changed `overlay_visible: false`
  - `Ctrl+Option+B` again changed `overlay_visible: true`

## Release

- Published artifact:
  - `dist/bluey-0.1.26-darwin-arm64.tar.gz`
- Artifact SHA256:
  - `697bde7408e8a6a06927bbe6f810b923f02d9eccce1f3fe420ac541a3aa63f67`
- Published to:
  - `root@165.227.77.152:/var/www/bluey`

## Current State

The local machine is running `bluey 0.1.26`, Bluey is visible, and capture exclusion remains enabled. The live downloadable macOS arm64 release includes the shortcut-scope and Ask-editing fix. Windows source parity was checked, but the live downloadable manifest still only publishes the macOS arm64 artifact.
