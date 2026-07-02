# Round 301 - Fresh Review Billing Guard Closure

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The user asked for a fresh end-to-end review of Codex-owned work to make sure
the implementation reflects the Bluey goal, especially the billing lessons
copied from Pinky.

## Review Finding

Round 300 blocked internal/admin/test accounts at the front doors:

- checkout
- saved payment method setup
- enabling Auto Reload
- Auto Reload availability in `/account/me`

Fresh review found one remaining edge:

- an internal/test account that already had Auto Reload enabled before the
  Round 300 guard could still be considered by the background Auto Reload
  worker

That did not match the goal. Billing safety needs to be enforced at the worker
as well as the UI/API.

## Fix

- Moved the internal/admin/test billing-account policy into
  `server/src/billing/policy.rs` so it can be shared by the API and background
  worker.
- Updated checkout and account settings to use the shared policy.
- Updated the Auto Reload worker to skip internal/admin/test accounts even if
  legacy DB state says Auto Reload is enabled and a payment method exists.
- Updated `/account/me` to report Auto Reload effectively off for
  internal/admin/test accounts, so the UI does not imply background reload is
  active.
- Added a regression test for the legacy case:
  - internal admin account
  - email verified
  - saved Stripe customer/payment method
  - Auto Reload already enabled
  - low balance
  - worker must skip before reserving an in-flight top-up

## Verification

Commands run:

```bash
cargo test --manifest-path server/Cargo.toml internal_and_test_accounts_cannot_enter_paid_billing_flows -- --nocapture
cargo test --manifest-path server/Cargo.toml skip_internal_test_account_even_if_auto_topup_was_already_enabled -- --nocapture
cargo test --manifest-path server/Cargo.toml billing -- --nocapture
cargo check --manifest-path server/Cargo.toml --quiet
git diff --check
```

All passed.

## Current State

Internal/admin/test accounts are now blocked from real billing setup and from
legacy background Auto Reload execution. The policy is centralized, so future
billing surfaces can use the same guard instead of copying rules.

## Remaining Gates

- Production deploy of the server API change still needs the proper server
  release path.
- Add an owner-facing reconciliation dashboard for ledger/provider/refund
  mismatches.
- Add durable Auto Reload attempt rows with provider ids, receipt state,
  idempotency, monthly spend guards, and failure reasons.
- Add checkout/reload consent evidence capture: terms version, IP, user agent,
  selected amount, threshold, and saved-payment consent.
