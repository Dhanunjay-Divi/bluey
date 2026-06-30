# Round 258 - Desktop Login Auto Link Deploy

## Trigger

The owner clicked the Bluey sign-in pill, completed browser login, and the desktop still did not reflect that it was logged in. The expected behavior is: `bluey on` should open login when needed, browser approval should signal back to the desktop, and Bluey should be ready without the user running extra commands.

## Root Cause

The macOS overlay sign-in button opened a plain web URL:

```text
https://bluey.sh/login
```

That authenticated the browser, but it did not create a device-code flow and the running daemon did not poll for approval or save tokens into the local Bluey account store.

`bluey login` already had the correct browser device-code flow, but `bluey on` and the overlay pill were not using it. The dashboard deep-link path also depends on a desktop app URL-scheme handler, which is not reliable for the CLI/native-overlay install path.

## Fix

Added a shared daemon login flow:

- New `DaemonRequest::CloudLogin`.
- New native overlay event `OverlayEvent::SignInRequested`.
- `bluey on` now starts `CloudLogin` automatically when no local Bluey token is linked.
- macOS overlay Sign in button now emits `sign_in_requested` instead of opening a plain login URL.
- Daemon starts `/auth/device/start`, opens `verification_uri?user_code=...`, polls `/auth/device/poll`, saves tokens with `save_account_profile_and_tokens`, enables cloud sync if needed, refreshes cloud status, starts balance polling, refreshes overlay balance, and shows `Bluey online`.
- Duplicate-login guard prevents repeated browser/device flows while one is already pending.

Security properties kept:

- Browser approval still requires the user to click `Connect desktop`.
- Random/pasted codes cannot silently bind an account.
- The desktop stores only its issued access/refresh tokens locally after approval.
- No provider keys are packaged into downloadable binaries.

## Windows Parity

The main fix is shared Rust CLI/daemon code, so Windows gets the `bluey on` auto-login path when a Windows package is built.

`open_browser_from_daemon` includes a Windows `cmd /C start` branch. `native/windows/cue-overlay/main.c` currently has no sign-in/login button equivalent to macOS, so there was no Windows overlay button to patch in this round.

## Verification

Local checks passed:

```bash
cargo test -p cue-core sign_in_event_serializes --quiet
cargo test -p cue-cli bluey_on_boot_lines_offer_browser_signin_when_unlinked --quiet
cargo test -p cue-daemon overlay_sign_in_event_is_accepted_by_production_validator --quiet
cargo check -p cue-cli -p cue-daemon --quiet
cargo fmt
swift build -c release --package-path native/macos/cue-overlay
git diff --check
```

Live daemon smoke, isolated from the real user profile:

```bash
HOME=/tmp/bluey-login-smoke-home...
PATH=/tmp/fake-open-bin:$PATH
BLUEY_CLOUD_API_URL=https://bluey.sh
./target/aarch64-apple-darwin/release/bluey-daemon --addr 127.0.0.1:57379 --no-overlay
printf '{"type":"cloud_login"}\n' | nc 127.0.0.1 57379
```

Daemon response:

```json
{"type":"text","text":"Opening Bluey sign-in in your browser."}
```

The fake `open` command prevented a real browser popup while still exercising the daemon path against the live auth server.

## Release/Deploy

Built macOS arm64 release with the embedded update public key:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
```

Published signed release metadata and artifact. The final public release version
is `0.1.18`; this intentionally bumps past `0.1.17` so existing installed
clients can auto-update on `bluey on`.

```bash
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem \
PUBLISH_DO=1 \
PUBLISH_HOST=root@165.227.77.152 \
PUBLISH_PATH=/var/www/bluey \
scripts/publish-bluey-release.sh
```

Artifact scan passed:

```text
Release artifact dev-flag/secret scan passed (12 files checked, no configured secrets present).
```

Live checks passed:

```text
https://bluey.sh/latest.json version: 0.1.18
latest.json.sig size: 88 bytes
OpenSSL: Signature Verified Successfully
darwin-arm64 sha256: 2272286b801c327e8c8f6913ce69ee76351367d6ea27aeef1341c6a03cf9eaab
```

Temp-home installer smoke from the live URL passed:

```bash
curl -fsSL https://bluey.sh/install.sh | bash
bluey 0.1.18
```

## Current State

Fresh installs and signed auto-update metadata are live on `bluey.sh` as
version `0.1.18`.

Expected user flow now:

```text
bluey on
-> Bluey starts
-> if not linked, browser opens to a device-code login URL
-> user signs in and clicks Connect desktop
-> daemon poll completes
-> tokens are saved locally
-> overlay switches to Bluey online and balance refreshes
```

## Remaining QA/Gates

- Run a manual real-browser approval smoke from a disposable account when convenient to verify the full final approval leg visually.
- Build and publish a Windows `0.1.18` artifact from the Windows build host if Windows downloads are needed for this exact round.
- If the user is already holding a stale/deleted local token, `bluey on` can still think the desktop is linked because local token presence is checked before server validation. A follow-up could validate `/account/me` on startup and auto-relink on `401`.
