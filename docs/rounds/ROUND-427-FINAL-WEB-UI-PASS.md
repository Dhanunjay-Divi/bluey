# Round 427 - Final Web UI Pass - 2026-07-08

## Goal
Do a final end-to-end Bluey web UI sweep for landing, account, billing, desktop connect, and session history after the Pinky-style polish rounds.

## Changes
- Sanitized billing and checkout error copy so restricted-account states cannot leak raw service wording into the web UI.
- Hardened the account balance and Auto Reload card layout so long copy wraps and narrower widths stack instead of overlapping.
- Kept normal web sign-in on the dashboard by replacing `/login` with `/account` after successful login, while leaving desktop deep-link handling on `/link`.
- Made desktop deep-link failures product-safe instead of exposing raw JavaScript errors.
- Made Session History detail rendering tolerate uploaded payloads shaped as `transcript`, `answers`, `responses`, `context`, or `files`, not only the strict internal bundle field names.

## Verification
- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Browser smoke with a local mock account:
  - landing hero/theme/footer loaded with no horizontal overflow
  - account dashboard loaded with no raw internal billing copy
  - balance and Auto Reload card did not overlap
  - My Computers hid browser sessions
  - remove desktop used the custom Bluey confirmation modal, not a native browser dialog
  - Billing had no stale credit-label, processor/backend, or `+ Auto Reload` CTA copy
  - Add Balance modal opened with positive guarded values
  - Session History listed only uploaded sessions and rendered transcript/answer/context detail when the API returned content

## Notes
- The web UI now displays uploaded session content when it is returned by the API. Capturing and syncing full local questions, responses, canvas, files, screen context, transcript, and voice remains desktop/backend runtime work.
