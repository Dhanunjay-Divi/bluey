# Round 260 - Update Restart and Listen Latency

## Trigger

The owner saw this after installing/updating:

```text
Bluey updated. Restarting Bluey...

Error: Bluey daemon did not become ready
```

They also reported that Listen was not quick and an answer showed a first visible response around `7.3s`.

## Root Cause

The installed local machine was still running/logging `0.1.18`, so some behavior was from the previous binary. The restart error also exposed real fragility in the update path:

- `bluey off` returned immediately after the daemon acknowledged shutdown, without waiting for the daemon process/IPC port to finish closing.
- `bluey on` waited only `5s` for the daemon to respond.
- The startup probe did not detect a daemon child process that exited before becoming ready.
- The auto-update relaunch used `bluey` from PATH instead of preferring the current installed executable path.

For Listen latency, Round 259 correctly added a `/account/me` verification before audio capture, but repeated Listen toggles could pay that verification round trip every time.

## Fix

- Increased the `bluey on` daemon readiness wait from `5s` to `20s`.
- Added child-process early-exit detection to the readiness wait.
- Made `bluey off` wait up to `8s` for the daemon to stop before returning.
- Made stale-daemon cleanup wait briefly after sending SIGTERM before removing stale state.
- Changed auto-update relaunch to prefer the current installed executable path, falling back to `bluey` from PATH only for local dev-target binaries.
- Added tests for the relaunch binary selection.
- Added a short in-daemon Listen account verification cache:
  - successful `/account/me` verification is cached for `30s`
  - logout clears the cache
  - failed verification clears the cache
  - browser/device login completion marks the cache fresh

## Security/Billing Impact

- Listen still fails closed before microphone/system audio capture if sign-in is missing, expired, deleted, insufficient, rate-limited, or temporarily unverifiable.
- The cache does not bypass first verification; it only avoids repeated verification calls during a short warm window.
- Deleted/expired account responses still clear local tokens and require sign-in again.

## Windows Parity

The restart hardening, installed-binary relaunch choice, shutdown wait, daemon readiness wait, and Listen auth cache are shared Rust CLI/daemon code. Windows packages inherit the same behavior when the next Windows artifact is built.

## Verification

Local checks passed:

```bash
cargo check -p cue-cli --quiet
cargo check -p cue-daemon --quiet
cargo test -p cue-cli relaunch_ --quiet
cargo test -p cue-daemon listen_auth_gate_requires_linked_cloud_account --quiet
```

Release/deploy checks:

```text
Release artifact dev-flag/secret scan passed.
https://bluey.sh/latest.json version: 0.1.20
latest.json.sig size: 88 bytes
OpenSSL: Signature Verified Successfully
darwin-arm64 sha256: bfa16fc62e45eaf7ed613a162fd3547b97616392143c710b78ac3e22d7980f2e
temp-home installer smoke: bluey 0.1.20
local install smoke: bluey 0.1.20 / bluey-daemon 0.1.20
local bluey on smoke: daemon ready with pid 70850
```

## Current State

Fresh installs and signed auto-update metadata are live on `bluey.sh` as version `0.1.20`.

The local machine was manually moved from `0.1.19` to `0.1.20` and `bluey on` started successfully after install.

## Remaining QA/Gates

- Answer latency still needs provider-route logs from the new binary if first visible token remains above target after the restart and auth-gate fixes.
- Windows `0.1.20` downloadable parity still requires a Windows build host/package.
