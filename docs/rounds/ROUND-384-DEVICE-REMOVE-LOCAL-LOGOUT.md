# Round 384 - Device Remove Logs Out Local Desktop

## Goal

When a user removes a computer from the Bluey web dashboard, that local Bluey desktop should be unlinked and signed out too.

## Problem

The Computers tab previously only revoked the dashboard device row. Desktop refresh tokens were separate from the stable device record, and the local daemon could keep showing/using the saved account until token expiry or manual logout.

## Changes

- Added `refresh_tokens.device_id` so desktop refresh tokens can be tied to the stable local device id.
- Device login now stores refresh tokens with the desktop `device_id`.
- Refresh-token rotation now preserves `device_id`, so a removed desktop cannot regain an unscoped refresh token.
- Removing one linked computer revokes only that device's refresh tokens.
- Removing all linked computers revokes all device-scoped refresh tokens for the account.
- Added `/account/devices/status` so the desktop can check whether its stable device id is still linked.
- The daemon balance poller now checks device status before balance refresh.
- If the device has been removed, the daemon clears local Bluey tokens and pushes the overlay back to signed-out state.
- Web remove confirmations and completion copy now say the action signs Bluey out on the desktop.

## Verification

- `cargo check --manifest-path server/Cargo.toml --bin bluey-server`
- `cargo check -p cue-daemon`
- `cargo check -p cue-cloud-client`
- `cargo test --manifest-path server/Cargo.toml refresh_tokens -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml devices -- --nocapture`
- `cargo test -p cue-daemon watch_clear_notifies_subscribers -- --nocapture`

## Notes

The local logout happens on the desktop's next authenticated balance poll. The default poll cadence is 30 seconds unless overridden by `BLUEY_BALANCE_POLL_SECS`.
