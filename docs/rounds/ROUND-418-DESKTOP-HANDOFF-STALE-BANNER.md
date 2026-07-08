# Round 418 - Desktop Handoff Stale Banner

## Trigger

The account page could keep showing `Finish desktop sign-in` after a desktop was already linked, especially when an old `user_code` remained in browser state or a reconnect kept the device count the same.

## Root Cause/Fix

- The web UI stored pending desktop codes in `sessionStorage`, but did not clear completed handoffs.
- Query params could revive the same old pending code on refresh.
- The post-approval poll only treated a higher linked-device count as success, so reconnecting the same desktop could appear stuck until refresh.

Fixes:

- Strip `user_code` and `device_code` from the URL after the web UI captures them.
- Preserve the original pending-code timestamp so old codes expire instead of getting refreshed forever.
- Clear pending desktop-code state after a successful handoff.
- Hide the normal connect prompt when the account already has a linked desktop and no fresh code is pending.
- Treat an approved handoff with an existing desktop row as connected, even when the linked-device count did not increase.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js web/index.html`

## Current State

Only web static files changed. Native overlay, audio, and backend runtime files were not touched.

## Remaining QA/Gates

- Live account check should confirm the stale banner disappears after the linked desktop list loads.
- Live reconnect check should confirm `Connect desktop` updates without requiring a manual refresh.
