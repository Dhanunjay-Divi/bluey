# Round 420 - Desktop Handoff Refresh State

## Trigger

After logging out, logging back in, or refreshing the account page, the dashboard could show `Bluey approved the desktop. It should appear here in a moment.` even when no Bluey desktop was present in the device list.

## Root Cause/Fix

- The web UI stored device approval state in `sessionStorage` as a permanent `bluey_device_approved_<code>` flag.
- Browser logout cleared account tokens, but not the pending desktop handoff state.
- On refresh, the dashboard trusted the old approval flag and restarted the "waiting for desktop" poll.

Fixes:

- Store desktop approval state with a timestamp instead of a permanent flag.
- Treat old legacy approval flags as stale.
- Expire approved handoffs after a short wait window.
- Clear pending desktop handoff state on browser sign-out/token clear.
- Redraw the desktop-link hint immediately when stale approval state is removed.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`

## Current State

Only web UI and round documentation files changed. Native overlay, audio, and backend runtime files were not touched.

## Remaining QA/Gates

- Live account check should confirm refresh no longer revives the stale approved-desktop banner.
- Fresh desktop connect should still show the short waiting state while the desktop checks in.
