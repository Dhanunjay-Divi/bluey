# Round 391 - Profile Logout Click Fix

## Goal

Make the profile-menu Logout action actually sign out from the Bluey web UI.

## Findings

- The shared profile menu stopped click propagation so clicks inside the menu never reached the document-level Logout handler.
- The old sign-out path waited for refresh/revoke work before clearing browser auth state, which could make logout feel stuck.

## Changes

- Handle `data-sign-out` clicks inside the profile-menu listener before stopping propagation.
- Clear local browser auth state immediately, then best-effort revoke the captured server refresh token.
- Kept the change scoped to the shared web UI script.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js docs/rounds/ROUND-391-PROFILE-LOGOUT-CLICK-FIX.md`
- `awk '/[ \t]$/{print FILENAME ":" FNR ": trailing whitespace"; bad=1} END{exit bad}' docs/rounds/ROUND-391-PROFILE-LOGOUT-CLICK-FIX.md`
