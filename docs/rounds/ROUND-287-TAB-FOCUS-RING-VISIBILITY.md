# Round 287 - Tab Focus Ring Visibility

## Trigger

The owner reported that after pressing Tab they could not find the highlighted cell/button in the Bluey overlay.

## Root Cause

Round 286 added a Bluey-managed keyboard focus ring, but the ring was initialized at `zPosition = 3000`. During normal layout, Bluey's fixed chrome is reasserted above content with z-positions around `4000+`:

- header and composer chrome at `4000`
- header controls around `4010`
- modal overlays above that

So Tab focus could move correctly, but the visual highlight could be painted underneath header/composer chrome and become invisible or too subtle.

## Fix

- Raised the macOS keyboard focus ring to `zPosition = 4900`.
- Reasserted that high z-position during layout so future chrome reordering does not bury it again.
- Increased ring border width from `1.8` to `3.0`.
- Increased ring shadow opacity/radius.
- Added a subtle blue focus fill so the selected button is obvious in dark and light themes.
- Bumped desktop workspace version to `0.1.41`.

## Mac/Windows Parity

This was a macOS-only visual layering bug. Windows already draws its focus outline directly inside the owner-drawn button in Round 286, so there is no equivalent sibling-layer z-order issue on Windows.

## Verification

Passed locally:

```bash
cargo check -p cue-daemon --quiet
swift build -c debug --package-path native/macos/cue-overlay
git diff --check
```

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/latest.json` reports version `0.1.41`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.41/bluey-0.1.41-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `23ca4c32d7781407616be9f9301b1ae38ef30f252ee45c0c73e96d6b417c7370`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.41` is live and downloadable from the droplet.
- Users should restart/update with `bluey off && bluey on` if they are still on `0.1.40`.

## Remaining QA

- Confirm that pressing `Tab` visibly highlights one Bluey button at a time.
- Confirm `Shift+Tab` moves backward.
- Confirm `Enter` / `Space` activates the visibly highlighted button.
