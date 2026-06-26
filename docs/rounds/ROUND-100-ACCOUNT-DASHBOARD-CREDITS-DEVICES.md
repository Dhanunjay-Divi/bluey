# Round 100 - Account Dashboard Credits And Devices

Date: 2026-06-22

## Goal

Make the account dashboard feel like the obvious place to manage credits first,
then show which desktop/browser sessions are linked to the account and let the
user remove them.

## Changes

- Grouped Add credits and Auto Reload at the top of the dashboard.
- Added a Linked devices panel below the credit controls.
- Added `GET /account/devices` for active, non-expired refresh sessions.
- Added `DELETE /account/devices/:device_id` to revoke one linked session for
  the authenticated account.
- Rendered device labels through DOM text nodes to avoid HTML injection.
- Added refresh-token tests for active session listing and scoped revocation.

## Notes

- Device ids are opaque refresh-token hashes. Raw refresh tokens are never
  returned to the browser.
- Removing a linked device revokes future refreshes. Any already-issued short
  access token may live until its normal expiry.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml db::refresh_tokens::tests`
- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.js web/assets/bluey-site.css server/src/db/refresh_tokens.rs server/src/api/account.rs server/src/api/mod.rs`
