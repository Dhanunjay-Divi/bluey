# Round 270 - Shortcut Hide Input Polish

## Trigger

The owner reported that Hide Bluey was not working reliably, the shortcuts panel did not need a Copy button, and the `T` shortcut should clearly mean text input.

## Root Cause

- macOS `Ctrl+Option+B` still used the older collapse/expand path, so hide behaved like a pill/minimize action instead of a true hide/restore.
- macOS relied mainly on `NSEvent` monitors for global shortcuts. That path could miss the shortcut depending on whether Bluey or another app owned focus.
- The macOS shortcuts panel reused the destructive-confirmation button layout and exposed a Copy action that was not useful.
- Shortcut copy still said `Focus Ask`, which was less clear than `Text input`.
- The macOS installer still mentioned the old F19 shortcut.

## Fix

- Added Carbon registered macOS hotkeys for:
  - `Ctrl+Option+B` hide/restore
  - `Ctrl+Option+T` text input
  - `Ctrl+Option+L` Listen
  - `Ctrl+Option+S` Screen
  - `Ctrl+Option+I` click-through/interactive
  - `Ctrl+Option+Enter` Answer
- Kept the existing event monitor as a fallback.
- Routed local key events through the same global shortcut path first, so shortcuts also work while Bluey itself has focus.
- Changed macOS hide/restore to fully hide all Bluey chrome and show the restore toast, then restore the full overlay on the next shortcut.
- Made local `T` open text input when Ask is not already focused.
- Removed the shortcuts Copy action from the macOS shortcuts panel.
- Updated shortcuts wording on macOS and Windows:
  - `Hide or restore Bluey`
  - `Text input`
- Updated Windows hide semantics for `Ctrl+Alt+B` and IPC `hide` to fully hide instead of only collapsing to the pill.
- Updated macOS install copy to say `Ctrl+Option+B` instead of F19.
- Bumped the desktop workspace to `0.1.24`.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `git diff --check`
- `swift build -c debug --package-path native/macos/cue-overlay`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=release bash native/macos/cue-overlay/build.sh`
- Local hot install into `~/.bluey/bin`, ad-hoc signed, restarted.
- Live shortcut smoke:
  - `Ctrl+Option+B` -> `overlay_visible: false`
  - `Ctrl+Option+B` again -> `overlay_visible: true`
  - `Ctrl+Option+T` did not create transcript/context rows and kept overlay visible.
- `make package-darwin-arm64`
- `scripts/publish-bluey-release.sh` dev-flag/secret scan passed.
- Live `https://bluey.sh/latest.json` reports version `0.1.24`.
- Live `latest.json.sig` verified successfully.
- Live `install.sh` serves the updated `Ctrl+Option+B` wording and no longer mentions F19.
- Local update installed `bluey 0.1.24`.

## Release

- Published artifact:
  - `dist/bluey-0.1.24-darwin-arm64.tar.gz`
- Artifact SHA256:
  - `b5a9cd1e65a0b6e45a29c4c8d8070e89361241cc61a8929363ee619135d0f2f7`
- Live install script SHA256:
  - `a2a931d23dc1f6d2b63e8717325971c8360569c2483bfd7c2e559f5d2b69d98c`
- Published to:
  - `root@165.227.77.152:/var/www/bluey`

## Current State

The local machine is running `bluey 0.1.24`; Bluey is visible and capture-excluded. macOS shortcut behavior is now backed by registered OS hotkeys, with monitor fallback. Windows source parity is implemented and syntax-checked, but the live downloadable manifest still only publishes the macOS arm64 artifact.

