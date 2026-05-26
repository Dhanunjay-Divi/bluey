# Bluey.sh Account Redesign - 2026-05-25

## Scope

Replaced the first-pass `/account` and `/reload` web surface with a console-style product UI.

## What Changed

- Converted the page from a large credits landing page into a compact app console.
- Added a left rail with Bluey identity, `bluey on` command context, and account sections.
- Added a top status row for API, checkout, and managed routing state.
- Added a command hero that explains the terminal-first flow without redirecting the user mentally away from Bluey.
- Reworked signed-out auth into a focused card with labeled inputs and clearer account copy.
- Added a signed-out preview card for wallet, auto-routing, and session knowledge behavior.
- Preserved all existing DOM IDs used by the auth, usage, and reload JavaScript.

## Verification

- `node` parsed every inline `<script>` block in `web/index.html`.
- `git diff --check` passed.
- Headless Chrome rendered `/account` locally at `1440x900` for visual review.

## Notes

- No backend code changed in this pass.
- Square checkout still requires deployment environment values for location IDs and webhook signature keys before reload can complete end-to-end.
