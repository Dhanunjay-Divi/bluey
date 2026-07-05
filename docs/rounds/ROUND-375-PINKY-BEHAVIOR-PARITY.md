# Round 375 - Pinky Behavior Parity

Date: 2026-07-05
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Audit Pinky's public/auth/download/account behavior and bring Bluey closer to the same interaction model while keeping Bluey colors and avoiding native/backend runtime changes.

## Changes

- Matched the account menu shape across landing, download, and policy pages:
  - Dashboard
  - Change Password
  - Delete Account
  - Logout
- Added the compact theme toggle to download and policy pages so the public nav behaves like Pinky across surfaces.
- Added `/account#password` and `/account#delete` hash handling so those menu actions open the matching dashboard modal.
- Added a Pinky-style `Remember me` checkbox on Bluey login.
  - Checked keeps the existing persistent browser login.
  - Unchecked stores auth tokens in session storage for the current browser session.
- Hid the desktop-code box from plain login unless the user arrived with a desktop code.
- Kept the signed-in dashboard connect-code card for linking a Bluey desktop.
- Changed password-reset copy from "reset link" to "reset email" so it is clearer without claiming Pinky's separate code-reset backend.
- Made admin trial-protection data lazy-load only when the Admin tab is selected by an admin account.
- Matched Pinky's copy feedback more closely with temporary `Copied!` state and temporary failure recovery.

## Pinky Audit Notes

- Landing:
  - Theme toggle flips light/dark.
  - Empty session-code Join is a no-op.
  - Try Us creates a real temporary trial, so it was inspected from Pinky code instead of clicked live.
- Download:
  - Platform cards switch one visible instruction panel.
  - Copy controls give short-lived feedback.
  - Linux is present but coming soon.
- Login:
  - Password eye toggles type.
  - Remember me is checked by default and can be toggled off.
  - Sign up and reset views live in the same compact auth page.
- Dashboard:
  - Account dropdown opens password/delete actions as modal flows.
  - Session history rows can open saved sessions separately.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css web/assets/bluey-site.js`
- Local browser click pass:
  - landing theme toggle
  - login password eye
  - Remember me toggle
  - signup view switch
  - password reset page
  - download platform panel
  - copy feedback fallback
- Live browser verification after deploy:
  - landing uses `v=2026070504` assets and "connect your account" copy
  - login shows Remember me and hides desktop-code entry by default
  - password reset says "Send reset email"
  - download shows logo, theme toggle, Login, macOS ready, Windows/Linux coming soon

## Notes

- No native overlay, audio, backend, or runtime files were edited.
- Existing unrelated dirty backend/runtime files were not staged.
