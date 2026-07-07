# Round 410 - Balance Language And Usage Estimates

Date: 2026-07-07

Scope:
- Keep changes limited to Bluey web UI and round documentation.
- Continue Pinky-style polish while keeping Bluey's prepaid wallet model clear.

Changes:
- Replaced dashboard Billing wording that made balance feel like a separate `Bluey credit` token.
- Changed the account flow from `Add credits` to `Add balance` where the user is managing money.
- Made the Billing inline card editor more compact and added a Cancel action.
- Added Usage Summary estimates:
  - default `$15` estimate for brand-new accounts,
  - recent-pace estimates once usage exists,
  - Auto Reload extension copy when Auto Reload is on.
- Added landing/account hints that `$15` is enough for many normal questions while screen/audio-heavy usage spends faster.

Validation:
- `node --check web/assets/bluey-site.js`

