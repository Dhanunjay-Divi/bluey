# Round 411 - In-App Balance Payment

Date: 2026-07-07

Scope:
- Keep the work focused on Bluey web account, billing, and payment UX.
- Add the smallest server billing route needed for the new one-step balance flow.
- Avoid native overlay, audio, and runtime files.

Changes:
- Added an in-app Square card payment route for one-time balance reloads.
- Let Add balance use the saved Auto Reload card when available.
- Let the first Add balance submit save a card for Auto Reload when Auto Reload is on.
- Kept hosted checkout as a fallback for accounts that cannot use the card form.
- Removed implementation wording from customer-facing billing, modal, policy, and checkout messages.
- Cleaned public FAQ and metadata copy so balance and payment language stays customer-facing.
- Reworked the Add balance modal so the bottom button is the only submit action.
- Removed Step 1/Step 2 wording from Add balance so it reads like a single checkout sheet.
- Switched balance and Auto Reload money fields away from native number spinners to prevent negative values from appearing.
- Changed public money copy from credits to balance where it refers to account funds.
- Sanitized payment error copy so implementation details stay out of the UI.
- Added an integration test for adding balance with a saved card while saving Auto Reload settings.

Validation:
- `node --check web/assets/bluey-site.js`
- `cargo check --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml square_pay_with_saved_card_adds_balance_and_updates_auto_reload`
- `cargo test --manifest-path server/Cargo.toml`
- `git diff --check`
