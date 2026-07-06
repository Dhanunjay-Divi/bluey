# Round 396 - Profile Logout Hardening

Date: 2026-07-06
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make the shared profile-menu `Logout` action reliably sign the browser out across landing, download, account, and policy pages.

## Changes

- Added a direct click handler on every `[data-sign-out]` button so logout does not depend on menu/document bubbling.
- Hardened `signOut()` so it immediately clears local and session auth, closes the profile menu, updates account chrome, and then performs the best-effort server revoke.
- Added an auth epoch/sign-out guard so a token refresh that was already in flight cannot restore the session after the user logs out.
- Bumped the static asset query version from `2026070505` to `2026070601` so deployed browsers fetch the fixed JS/CSS.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local browser harness: clicking `Logout` on `/download` hides the profile menu and shows the guest login state.
