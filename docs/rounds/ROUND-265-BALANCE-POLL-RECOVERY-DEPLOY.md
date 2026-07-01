# Round 265 - Balance Poll Recovery Deploy

## Trigger

The overlay showed `Balance --` while the same installed CLI could fetch the linked account and credits successfully. The current linked account reported a real balance of `$14.56`, so the server/account path was alive and the stale state was inside the long-running desktop daemon.

## Root Cause

The balance poller owns a long-lived cloud client. Browser login, CLI login, or one-off CLI token refresh can update the secure token store, but an already-running balance poller kept using its old in-memory token cache. After enough failures it stopped emitting balance snapshots, leaving the overlay with `--`.

## Fix

- Added `CloudClient::reload_tokens_from_store()` to refresh the in-memory token cache from the persistent token store.
- Updated daemon balance polling to reload stored tokens and retry immediately after a poll error.
- Kept the retry path logged with safe error strings so future reports can tell whether balance polling failed before or after token reload.
- Bumped the desktop release version to `0.1.22` so `bluey on` and fresh installs can receive the fix through the signed release manifest.

## Verification

- `cargo fmt`
- `cargo test -p cue-cloud-client reload_tokens_from_store_refreshes_cached_tokens -- --nocapture`
- `cargo check -p cue-daemon -p cue-cli --quiet`
- `cargo update --manifest-path server/Cargo.toml -p cue-core`

## Deployment

- Built `dist/bluey-0.1.22-darwin-arm64.tar.gz` with the embedded release public key.
- Published through `scripts/publish-bluey-release.sh` using the Bluey Ed25519 release key.
- Release artifact scan passed:
  - no configured secret values found in the archive
  - no capture-visible/dev overlay flags found in the archive
- Live `https://bluey.sh/latest.json` reports `0.1.22`.
- Live `latest.json.sig` is 88 bytes and verifies with OpenSSL against the release public key.
- Live artifact checksum matches `latest.json`.
- Temp-home installer smoke from `https://bluey.sh/install.sh` installed `bluey 0.1.22`.
- Local machine was updated to `bluey 0.1.22`, restarted, and `bluey credits` returned `Balance: $14.56`.

## Server Rollout

The previous Round 264 server data-ops gates were also rolled to the droplet in this pass:

- Synced server source to `/opt/bluey-build-codex-round265` without `target/` build outputs.
- Updated the droplet root Rust stable toolchain because system Cargo `1.75.0` cannot parse Cargo.lock v4.
- Built the Linux `bluey-server` release binary on the droplet.
- Backed up the previous binary to `/var/backups/bluey-api/bin/bluey-server.previous`.
- Installed the new binary to `/usr/local/bin/bluey-server`.
- Restarted `bluey-api.service`.
- Live health checks passed:
  - `systemctl is-active bluey-api.service` -> `active`
  - `http://127.0.0.1:8080/health` -> `status=ok`, `commit=0a3c67e`
  - `https://bluey.sh/health` -> `status=ok`, `commit=0a3c67e`

## Current State

The account server is able to return the balance. This round fixes the desktop-side recovery path that caused the overlay to keep showing `--` after auth state changed under a running daemon.

Startup cloud object sync showed fail-closed warnings for old local capture artifacts during the restart window, but `/account/me` returned `200` and `bluey credits` returned the expected balance after token refresh. The object-sync warnings did not continue advancing after startup.
