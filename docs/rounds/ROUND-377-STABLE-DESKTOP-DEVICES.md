# Round 377 - Stable Desktop Devices

## Trigger

The account dashboard showed repeated `Browser session` and `Bluey desktop` rows after the same laptop was linked multiple times. The owner expected Pinky-style behavior: one stable computer row per installed host, updated on reconnect.

## Root Cause

`/account/devices` was rendering active refresh-token sessions as computers. Every browser session, desktop token exchange, or repeated login could become another dashboard row because there was no stable desktop-device identity keyed to the account.

## Fix

- Added `account_devices`, keyed by `(account_id, device_id)`, for stable linked desktops.
- Added device metadata to the OAuth-style device-code login row.
- On device-code approval and poll, Bluey now upserts the desktop row before issuing desktop tokens.
- Added authenticated `/account/devices/register` for post-login registration refreshes.
- Bluey desktop now preserves a stable local ID at `~/.bluey/device_id` and stores the same ID in the account profile.
- Dashboard computers now render stable desktop records instead of refresh-token/browser sessions.

## Verification

- `cargo test --manifest-path server/Cargo.toml device -- --nocapture`
- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `cargo check -p cue-cloud-client`
- `cargo check -p cue-daemon`
- `node --check web/assets/bluey-site.js`

## Current State

Repeated connect/login from the same laptop should update the existing linked-device row. Browser sessions no longer appear as computers. Existing duplicate rows from old refresh-token-derived data disappear because the Computers tab now reads the new stable device table.

## Remaining QA/Gates

- Deploy the server migration/API change before expecting live dashboard data to switch over.
- Ship the desktop build with the stable `device_id` sender so future connections populate the stable table.
- A later round can add periodic device heartbeats if the dashboard needs live/offline status beyond login-time freshness.
