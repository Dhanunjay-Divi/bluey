# Round 398 - Compact Computers Connect

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make the Computers tab feel cleaner by separating the desktop connect flow from the linked desktop list and keeping session/history language out of the computers empty state.

## Changes

- Split the Computers tab into two separate cards:
  - compact `Connect Bluey desktop`
  - `My Computers` desktop list
- Reduced the connect-code panel height, padding, button sizing, and form spacing.
- Updated the Computers list copy to clarify that saved chats stay in Session History.
- Changed the no-desktops empty state from `session-empty` to `device-empty`.
- Bumped the web asset query version to `2026070701`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local browser harness with an authenticated mock account:
  - Connect section rendered as a compact separate card.
  - Device list rendered as a separate card.
  - Empty device state used `device-empty`.
  - `2026070701` CSS asset loaded.
