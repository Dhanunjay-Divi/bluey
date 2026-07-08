# Round 408 - Billing Negative Input Guards

Date: 2026-07-07
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: main

## Goal

Prevent negative reload or Auto Reload values from appearing in the web UI, and confirm that negative billing values cannot affect payment or ledger state.

## What Changed

- Added web UI sanitizers for dollar inputs in `web/assets/bluey-site.js`.
- Manual reload amount now strips minus signs and clamps to the valid range:
  - minimum: `$15`
  - maximum: `$500`
- Auto Reload draft fields now strip minus signs and clamp to valid ranges:
  - threshold minimum: `$1`
  - threshold maximum: `$50`
  - reload amount minimum: `$15`
  - reload amount maximum: `$500`
- Checkout and Save read paths now clamp the visible field values before reading them, so skipped browser input events still cannot leave negative UI state.
- Added backend regression coverage for negative checkout amounts.
- Extended Auto Reload settings tests to reject negative reload amount and negative threshold values.

## Payment-Side Behavior

The backend does not turn negative values into zero or silently grant credits. Bad values are rejected before payment provider calls:

- `/billing/checkout` rejects amounts below the `$15` minimum with `400`.
- `/account/billing` rejects invalid Auto Reload settings with `400`.
- Negative UI drafts are not saved when Auto Reload is off.

This means a negative value cannot create a Square/Stripe checkout, cannot save an Auto Reload setting, and cannot credit the user.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e billing_checkout_rejects_negative_reload_before_provider_call -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e square_auto_reload_requires_saved_card_then_enables -- --nocapture`
- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Static smoke confirmed the new UI sanitizer and backend tests are present.
- Local web preview loaded `/account`; the billing panel is auth-gated, so focused static/live asset checks were used for the sanitizer.
- Live asset check confirmed `https://bluey.sh/assets/bluey-site.js` contains the sanitizer hooks.
- `scripts/bluey-release-live-verify.sh 0.1.90`

## Deployment Notes

Deployed the hosted web UI to `bluey.sh`. Backend behavior was already fail-closed; backend code was not changed in this round, but tests now lock that behavior.
