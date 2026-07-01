# Round 279 - Desktop Login And Uninstall

## Trigger

The desktop login flow still felt broken in the live tester path:

- `bluey on` opened a browser page, but terminal output still showed a generic login URL instead of the one-time desktop-code URL.
- Clicking the overlay `Open login` action could return `Bluey sign-in is already open in your browser` without reopening anything.
- `/login?user_code=...` could look like a normal login page instead of an obvious desktop-connect flow.
- `bluey uninstall` did not exist.
- `curl -fsSL https://bluey.sh/install.sh | bash` had previously returned the HTML app shell when the installer file was missing or not served as a real file.

## Root Cause

- The daemon stored only a background login `JoinHandle`, not the active login URL/code, so it could not reopen the current device-code URL on retry.
- `bluey on` printed the static `https://bluey.sh/login` fallback after starting the real device flow.
- The web account page read the device code only from the current URL query string; the code was not preserved as a first-class pending desktop-link state.
- Installer files were served by the same static fallback path as the single-page website if the release root file was missing.
- The CLI had install/update/logout/delete commands but no product-level uninstall command.

## Fix

- Reworked daemon desktop login state:
  - stores the active login URL, user code, start time, and background polling task
  - repeated `CloudLogin` or overlay sign-in requests reopen the same one-time URL
  - close-together duplicate sign-in requests abort the losing task and reuse the active flow
  - login cards now include `login_url: ...` so the overlay can show an actionable login card
- Changed device login URLs to include `desktop=1&user_code=...`.
- Stopped `bluey on` from printing the generic `/login` URL after it requests a real device-code login.
- Updated the web account page:
  - remembers a pending desktop code in session storage for the 10-minute device-flow lifetime
  - keeps the `Connect desktop` banner visible before and after account auth
  - copy now says the desktop code is already filled from the app
  - retains the explicit `Connect desktop` click for account-binding safety
- Added `bluey uninstall`:
  - stops Bluey first
  - removes known install roots and CLI symlinks on macOS/Windows paths
  - keeps local account data and saved sessions by default
  - `--purge-data` removes local tokens/settings/sessions/logs/runtime state
  - refuses to treat local repo/debug builds as install roots
- Updated macOS and Windows installer copy to mention `bluey uninstall`.
- Hardened the Caddy example so `/install.sh`, `/install.ps1`, `latest.json`, and `/releases/*` are served as real files, not via the HTML fallback.
- Bumped desktop workspace version to `0.1.33`.

## Verification

Passed locally:

```bash
node --check web/assets/bluey-site.js
cargo test -p cue-cli device_login_url --quiet
cargo test -p cue-cli uninstall_root_detection --quiet
cargo check -p cue-cli --quiet
cargo check -p cue-daemon --quiet
cargo run -p cue-cli --bin bluey --quiet -- uninstall --help
cargo test -p cue-daemon overlay_sign_in_event_is_accepted_by_production_validator --quiet
```

Passed release/deploy checks:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

- Live `https://bluey.sh/latest.json` reports `0.1.33`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live macOS artifact:
  `https://bluey.sh/releases/v0.1.33/bluey-0.1.33-darwin-arm64.tar.gz`
- Live SHA256:
  `d0d6ff6059eff0f32ee61ec93fb9777bc1d6a1a0c4b6e24a885d62916ef6d183`
- Live `/install.sh` returns `application/x-shellscript` and begins with `#!/usr/bin/env bash`.
- Live `/install.ps1` returns `application/x-powershell`.
- Live account bundle contains the pending desktop-code storage and `Connect desktop` copy.
- Temp-root installer smoke passed:
  - downloaded `0.1.33`
  - checksum verified
  - installed CLI reports `bluey 0.1.33`
  - installed CLI exposes `bluey uninstall --help`

## Current State

Closed and deployed:

- Published `v0.1.33` release artifacts to `https://bluey.sh`.
- Synced updated web static bundle to `https://bluey.sh`.
- Updated live Caddy config with a dedicated `@bluey_install` handler for `/install.sh`, `/install.ps1`, `latest.json`, `latest.json.sig`, and `/releases/*`.
- Validated and reloaded Caddy.
- Verified live `/install.sh` is shell, not HTML.
- Verified live `/login?desktop=1&user_code=TEST-CODE` serves the updated account bundle.
- Smoke-tested install/uninstall help from a temp install root.

Remaining QA:

- Run a real signed-out desktop login on a user machine to confirm the browser account page reflects the device code after auth and the daemon receives the completed link.

## Windows Parity

- `bluey uninstall` is cross-platform and includes the Windows install root and `.exe` link candidates.
- Windows installer copy now mentions `bluey uninstall`.
- No Windows overlay source changed in this round.
