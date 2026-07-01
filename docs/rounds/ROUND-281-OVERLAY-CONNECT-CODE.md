# Round 281 - Overlay Connect Code

## Trigger

The desktop login flow still made the user look at Terminal for the one-time code. The owner asked for the connect code to be shown in the overlay as well, so a production user can complete sign-in from the Bluey window without understanding Terminal output.

## Root Cause

The daemon already knew the device `user_code`, but the overlay login card treated it as plain body text. The sign-in card had an action, but it did not promote the code as a first-class visual step.

## Fix

- Changed the daemon login card body to a structured production-friendly shape:
  - instruction line
  - `Code: XXXX-XXXX`
  - finish line
  - hidden `login_url: ...`
- Updated the macOS overlay sign-in card:
  - extracts the connect code from `Code:` or from the login URL query
  - shows a dedicated `Connect code` pill with the monospaced code
  - hides raw `Code:` and `login_url:` metadata from the body copy
  - changes the CTA from `Open login` to `Open browser`
- Windows parity:
  - the daemon now sends the structured `Code:` line to all clients
  - the current Windows overlay body renderer will show that line plainly even before custom pill styling

## Verification

Passed locally:

```bash
cargo fmt --all
cargo check -p cue-daemon --quiet
cargo check -p cue-cli --quiet
swift build -c debug --package-path native/macos/cue-overlay
/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
```

Passed release/deploy checks:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

- Live `https://bluey.sh/latest.json` reports `0.1.36`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live macOS artifact:
  `https://bluey.sh/releases/v0.1.36/bluey-0.1.36-darwin-arm64.tar.gz`
- Live SHA256:
  `4c0f90d748019ad05fb5a520e4775d77ab58e8350dde692cb93923347968e0b5`
- Live `/install.sh` returns `application/x-shellscript`.
- Live `/install.ps1` returns `application/x-powershell`.
- Temp-root installer smoke passed:
  - downloaded `0.1.36`
  - checksum verified
  - installed CLI reports `bluey 0.1.36`

## Current State

Closed and deployed:

- `v0.1.36` is the current public production release.
- A signed-out macOS overlay login card should now show:
  - instruction copy
  - a `Connect code` pill
  - `Open browser` CTA
- Windows receives the same structured daemon body and shows the `Code:` line through the current body renderer.
