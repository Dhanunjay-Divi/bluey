# Round 286 - Shortcut Parity And Button Keyboard Navigation

## Trigger

The owner shared a video showing the shortcut panel changing between click-through states and asked for the commands to stay aligned. They also asked for Tab to select the next Bluey icon/button and Enter to open the selected control when the overlay is interactive.

## Root Cause

The shortcut guide had two different bodies: click-through off showed inside keys plus globals, while click-through on showed only global shortcuts. History and Files also existed only as local inside keys, so the help text and actual global behavior did not match.

On macOS, key routing sent most unhandled local keys into the composer. That meant Tab could land in the ask box instead of moving between controls, and Enter could fall through to Answer while a help/confirm panel was open. The shortcut help body could also behave like selectable text, which caused the white selection block seen in the video.

On Windows, the shortcut pop-up had the same mode-dependent wording, History/Files were not registered global hotkeys, and owner-drawn buttons did not have a Bluey-managed Tab focus loop.

## Fix

- Made the shortcut guide stable across click-through states:
  - always shows the same inside Bluey commands
  - always shows the same global commands
  - only the short mode note changes between click-through on/off
- Added global History and Files shortcuts:
  - macOS: `Ctrl+Option+H` and `Ctrl+Option+F`
  - Windows: `Ctrl+Alt+H` and `Ctrl+Alt+F`
- macOS overlay:
  - added a Bluey-managed keyboard focus ring for visible enabled buttons
  - `Tab` / `Shift+Tab` cycles buttons when click-through is off
  - `Enter` / `Space` activates the selected button
  - modal help/confirm sheets keep local keys inside the modal
  - shortcut help text is explicitly non-editable and non-selectable
  - clicking/mousing in the overlay clears keyboard button focus
- Windows overlay:
  - added a visible focus outline for owner-drawn buttons
  - `Tab` / `Shift+Tab` cycles visible enabled buttons when interactive mode is on
  - `Enter` / `Space` activates the focused button
  - registered/unregistered History and Files global hotkeys
- Bumped desktop workspace version to `0.1.40`.

## Verification

Passed locally:

```bash
cargo check -p cue-daemon --quiet
swift build -c debug --package-path native/macos/cue-overlay
/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/latest.json` reports version `0.1.40`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.40/bluey-0.1.40-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `e1551312aaba717e8bac446995297ddc26a55ad11a135e5218bb145783fc97d8`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.40` is live and downloadable from the droplet.
- Windows source parity is implemented and syntax-checked.
- Live public `latest.json` still advertises `darwin-arm64` only, so Windows needs the Windows build/package host before these Windows changes become a downloadable Windows artifact.

## Remaining QA

- On macOS after update:
  - open Keyboard shortcuts with click-through off and on, confirm command list stays aligned
  - press `Tab` / `Shift+Tab`, confirm one visible button is highlighted at a time
  - press `Enter` on selected buttons, confirm activation
  - confirm shortcut help text no longer creates text selection blocks
  - confirm `Ctrl+Option+H/F` opens History/Files and does not hide/close Bluey
- On Windows build host:
  - package and smoke-test `0.1.40` Windows artifact
  - confirm `Ctrl+Alt+H/F`, Tab focus, and Enter activation
