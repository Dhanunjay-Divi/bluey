# Round 292 - Stale Ask Focus Shortcut Keys

## Trigger

The owner reported that individual shortcut keys were still getting typed into the Ask input box.

## Root Cause

Round 291 removed the broad printable-key autofocus fallback, but one edge remained:

- If Ask was already focused from an earlier click or shortcut, macOS correctly treated later printable keys as text input.
- That meant reserved local shortcut keys such as `L`, `S`, `I`, `H`, `F`, and `T` could still leak into an empty Ask box if Ask had stale focus.

## Fix

- Added a stale-empty-Ask shortcut path for macOS local shortcuts.
- When click-through is off and Ask is focused but empty:
  - if Ask was not just deliberately armed for typing, reserved local shortcut keys route to Bluey instead of being typed
  - `L` starts/stops Listen
  - `S` captures/analyzes screen
  - `I` toggles click-through
  - `H` opens History
  - `F` opens Files/attachments
  - `T` intentionally focuses Ask again
- Added a short typing grace window after intentionally focusing Ask, so clicking Ask or pressing `T` still lets the user start typing normally.
- Bumped desktop workspace version to `0.1.46`.

## Mac/Windows Parity

This was a macOS focus-state edge. Windows already gates local shortcuts around native edit focus and does not have the same stale `NSTextView` routing. Windows source was syntax-checked again.

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

- `https://bluey.sh/latest.json` reports version `0.1.46`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.46/bluey-0.1.46-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `6bd87be6218df20292907c203856101cecc937eefc10dda234dc5b58ec6d426f`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.46` is live and downloadable from the droplet.
- Windows source remains syntax-checked.

## Remaining QA

- Owner should run `bluey off && bluey on`, confirm update to `0.1.46`, then test:
  - Leave Ask focused and empty for a moment, then press `L`; Listen should start/stop.
  - Press `S`, `I`, `H`, and `F`; they should trigger their Bluey actions instead of typing.
  - Click Ask and immediately type a normal prompt; typing should still work.
