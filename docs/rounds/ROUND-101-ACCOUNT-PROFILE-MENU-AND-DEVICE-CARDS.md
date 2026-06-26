# Round 101 - Account Profile Menu And Device Cards

Date: 2026-06-22

## Goal

Bring the Bluey dashboard closer to the Pinky account-management feel while
keeping account security actions real and easy to find.

## Changes

- Added a profile icon in the account top bar.
- Added a profile dropdown with email, Change password, Sign out, and Delete
  account.
- Added a linked-device summary bar with device count, Refresh, and Remove all.
- Restyled linked-device rows into larger machine cards with status, last-active
  metadata, and red Remove buttons.
- Added `DELETE /account/devices` to revoke all active refresh sessions for the
  authenticated account.
- Kept sign-out resilient by clearing local browser tokens even when a refresh
  or logout call fails.

## Notes

- The devices panel is backed by refresh sessions. Removing a row or removing
  all rows revokes future refreshes; already-issued short access tokens can
  remain valid until their normal expiry.
- Device ids remain opaque hashed refresh-token ids. Raw tokens are not exposed
  to the browser.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml db::refresh_tokens::tests`
- `node --check web/assets/bluey-site.js`
