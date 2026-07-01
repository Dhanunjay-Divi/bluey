# Round 276 - Remove Legacy H Hide Toast

## Trigger

The owner reported that `Ctrl+Option+H` was still doing a complete hide path and that Bluey could show the old toast:

```text
Bluey hidden - press Ctrl+Option+B to restore
```

They also asked whether the pill/overlay is hidden from screen share or visible.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

- The Tauri dashboard still registered `Ctrl+Alt+H` / `Ctrl+Option+H` as a legacy global overlay toggle.
- macOS native overlay source still carried the old full-hide restore-toast helper even though Round 275 changed normal Hide to pill collapse.
- Installer copy still said `hide/restore`, which could reinforce the old full-hide mental model.

## Fix

- Removed the dashboard `Ctrl+Alt+H` global shortcut registration.
- Kept `H` as an inside-overlay History shortcut only when the native overlay itself is interactive and Ask is not focused.
- Removed the old macOS `RestoreToast`, `hideAllBlueyChrome`, and fade-hide helper path from active overlay source.
- Updated installer copy to say `Ctrl+Option+B to minimize/restore`.
- Bumped the desktop workspace version to `0.1.30`.

## Screen-Share Behavior

In normal production mode, the pill and expanded overlay remain capture-excluded. `bluey status` should show:

```json
"overlay_capture_excluded": true
```

That means Bluey should not appear in normal screenshots or screen-share capture. The intended exception is local QA visible mode, which is only enabled with the explicit capture-visible dev flags.

## Verification

- `cargo check -p cue-daemon --offline`
- `swift build -c debug --package-path native/macos/cue-overlay`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon --lib --locked`
  - `281 passed; 0 failed; 2 ignored`
- `cargo check -p cue-dashboard --locked`
- `cargo test -p cue-core sign_in_event_serializes --locked`
- Source search confirmed no active `RestoreToast`, `Bluey hidden`, `hideAllBlueyChrome`, `Ctrl+Alt+H`, or `Ctrl+Option+H` remains in active source paths.
- Published `v0.1.30` to `https://bluey.sh/latest.json`.
- `latest.json.sig` verified successfully.
- `https://bluey.sh/install.sh` serves `application/x-shellscript`.
- Local update installed `bluey 0.1.30`.
- Local status after restart showed `"overlay_capture_excluded": true`.
- Local macOS shortcut smoke:
  - `Ctrl+Option+B` restored from collapsed state.
  - `Ctrl+Option+H` did not change Bluey overlay state.

## Current State

`Ctrl+Option+H` should no longer be a global Bluey shortcut. `Ctrl+Option+B` remains the global minimize-to-pill / restore shortcut on macOS. The old complete-hide toast path is removed from the native macOS overlay source.

## Live Artifact

```text
https://bluey.sh/releases/v0.1.30/bluey-0.1.30-darwin-arm64.tar.gz
SHA256: 5f39eb29f3f1f0c25e08ecc334508fa67a1d7c2983798ee5a5c5081d3b426eff
```

## Remaining QA / Gates

- If a separately installed dashboard app is still running from an old build, it may need restart/update to release the old `Ctrl+Option+H` registration.
