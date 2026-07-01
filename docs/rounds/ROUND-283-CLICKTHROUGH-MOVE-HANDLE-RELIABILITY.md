# Round 283 - Click-Through Move Handle Reliability

## Trigger

The owner reported that the position control during click-through-on mode was not working at all. In that mode blank Bluey space must click through to the app behind it, but the blue move handle must still reliably drag the overlay.

## Root Cause

The macOS expanded window is intentionally toggled between mouse-active and mouse-ignored while click-through is on. The move handle existed, but the window-level mouse policy depended on normal interactive hit testing to wake the window before mouse-down. If that policy missed the small handle region, the whole window stayed ignored and the handle felt dead.

## Fix

- Made the macOS click-through move handle larger.
- Increased the handle hitbox so it is usable at speed.
- Added a direct screen-space `moveHandleContainsScreenPoint` check in the macOS overlay.
- Updated the window-level mouse policy so the move handle wakes the click-through window before click/drag begins.
- Added a global mouse fallback for click-through-on mode:
  - mouse movement over Bluey refreshes the pass-through policy
  - a press on the move handle arms a manual drag even if the window was still mouse-ignored at mouse-down time
  - drag/up events move and persist the expanded window frame
- Kept blank-space click-through behavior unchanged.
- Windows parity:
  - increased the cyan move handle size
  - inflated the Windows `HTCAPTION` hitbox for the handle
- Bumped desktop workspace version to `0.1.38`.

## Verification

Passed locally:

```bash
cargo fmt --all
swift build -c debug --package-path native/macos/cue-overlay
/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
cargo check -p cue-daemon --quiet
cargo test -p cue-daemon pcm16_i16le_stats_detect_silence_and_audible_samples --quiet
cargo test --manifest-path server/Cargo.toml stt::tests --quiet
```

Passed release/deploy checks:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

- Live `https://bluey.sh/latest.json` reports `0.1.38`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live macOS artifact:
  `https://bluey.sh/releases/v0.1.38/bluey-0.1.38-darwin-arm64.tar.gz`
- Live SHA256:
  `480722b769522f43051b1056fc2e3d63395e5545bb62b341c6b711e7b2bc7669`
- Live `/install.sh` returned `application/x-shellscript`.
- Live `/install.ps1` returned `application/x-powershell`.

## Current State

- `v0.1.38` is published on `bluey.sh`.
- Users should receive this build through the signed updater/installer path.

## Remaining QA

- Install `0.1.38`, turn click-through on, hover the blue move handle, then drag.
- Confirm blank header/body/composer space still clicks through to the app behind Bluey.
