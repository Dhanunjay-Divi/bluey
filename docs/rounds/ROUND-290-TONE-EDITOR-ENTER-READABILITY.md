# Round 290 - Tone Editor Enter and Readability

## Trigger

The owner reported that when the Tone editor is open, pressing `Enter` should save it. The owner also asked for the Tone input to start at the left instead of visually centered, and for the `How should Bluey answer?` title to be bright/visible instead of grey.

## Root Cause

- The macOS Tone editor delegate handled Escape, but did not handle field-editor newline commands, so `Enter` did not save consistently from inside the Tone text field.
- The Tone text field was center-aligned, which made typed text feel like it started in the middle.
- The Tone title used dim text styling, which made the modal title look disabled.

## Fix

- Added macOS Tone field handling for:
  - `insertNewline:`
  - `insertNewlineIgnoringFieldEditor:`
- Both commands now call the same save path as the Save button.
- Changed the Tone title copy to `How should Bluey answer?`.
- Made the Tone title bright:
  - white in dark mode
  - normal text color in light mode
- Changed the Tone input alignment from centered to left.
- Bumped desktop workspace version to `0.1.44`.

## Mac/Windows Parity

This was a macOS-native Tone modal polish fix. The Windows overlay does not have the same native Tone input modal in this file; its Style button emits `instructions_requested` for the daemon/app flow. Windows source was syntax-checked again to make sure this round did not break cross-platform overlay builds.

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

- `https://bluey.sh/latest.json` reports version `0.1.44`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.44/bluey-0.1.44-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `09fc42a591cd7279b11fc276c0c0058be6a100a528eac4f1b47e8b52e2217fc2`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.44` is live and downloadable from the droplet.
- Windows source remains syntax-checked.

## Remaining QA

- Owner should run `bluey off && bluey on`, confirm update to `0.1.44`, then test:
  - Open Tone.
  - Type a style.
  - Press `Enter`.
  - Confirm the modal saves/closes.
  - Confirm text starts from the left and the title is clearly visible.
