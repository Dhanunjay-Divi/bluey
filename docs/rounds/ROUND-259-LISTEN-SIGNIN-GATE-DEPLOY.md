# Round 259 - Listen Sign-In Gate Deploy

## Trigger

The owner clicked the mic/Listen control before desktop sign-in completed and Bluey appeared to start listening. The required behavior is strict: no mic, live transcription, STT usage, or billing path should start until the desktop account is signed in and verified.

## Root Cause

Round 258 fixed the browser-to-desktop login flow, but Listen still had two weaker spots:

- The overlay optimistically changed its own UI to `Starting` before the daemon accepted the recording request.
- The daemon could reach the audio start path with only local token presence checked later by the STT path, so stale/deleted local tokens could briefly enter capture setup before `/account/me` rejected them.

## Fix

Added a daemon-side Listen auth gate before any audio capture starts:

- `AudioStart` IPC and overlay `recording_start_requested` now call `verify_cloud_account_for_listen` first.
- The gate requires a buildable Bluey cloud client and a successful `/account/me` verification.
- If no token is present, Listen stays off and the daemon opens the browser device sign-in flow.
- If the token is expired, unauthorized, or points at a deleted account (`401`, `403`, `404`), Bluey clears local tokens, keeps Listen off, and opens sign-in again.
- If the account is verified but lacks credits, is cooling down, or the account check times out, Listen stays off without claiming sign-in is opening.
- The macOS overlay no longer switches the expanded or collapsed Listen UI into `Starting` until the daemon sends the accepted listening state.

## Security/Billing Impact

- Unsigned users cannot start microphone/system audio capture from either the expanded overlay, collapsed pill, or direct daemon IPC.
- Deleted-account/stale-token desktops cannot keep streaming against a removed account.
- No STT reservation or provider call should occur before account verification passes.

## Windows Parity

The main protection is shared Rust daemon code, so Windows gets the same `AudioStart`/recording-event gate when packaged. The macOS-specific change only removes optimistic local UI state in the native macOS overlay; Windows parity is covered by the daemon-level refusal because the Windows overlay also talks to the daemon.

## Verification

Local checks passed:

```bash
cargo test -p cue-daemon listen_auth_gate --quiet
cargo test -p cue-cli bluey_on_boot_lines_offer_browser_signin_when_unlinked --quiet
cargo check -p cue-daemon --quiet
swift build -c release --package-path native/macos/cue-overlay
```

Release/deploy checks:

```text
Release artifact dev-flag/secret scan passed (12 files checked, no configured secrets present).
https://bluey.sh/latest.json version: 0.1.19
latest.json.sig size: 88 bytes
OpenSSL: Signature Verified Successfully
darwin-arm64 sha256: ebc2f52e07faf40ef7e09f167fcb804c60f00f7ebba8875d528567289b84c885
temp-home installer smoke: bluey 0.1.19
```

## Current State

Fresh installs and signed auto-update metadata are live on `bluey.sh` as version `0.1.19`, so existing `0.1.18` installs can update.

## Remaining QA/Gates

- Run a real unsigned desktop smoke: clear local tokens, start `bluey on`, click Listen, verify the browser sign-in starts and the overlay does not enter Listening.
- Build/publish Windows `0.1.19` from a Windows build host if exact Windows downloadable parity is needed.
