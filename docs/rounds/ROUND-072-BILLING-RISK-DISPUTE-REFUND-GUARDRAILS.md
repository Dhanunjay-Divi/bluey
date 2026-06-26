# Round 072 - Billing Risk, Dispute, and Refund Guardrails - 2026-06-20

## Goal

Close the Bluey money-path gap where a refunded or disputed processor payment could leave a customer account able to keep spending Bluey credits, keep auto-reload enabled, or keep a saved off-session card/payment method active.

This round follows the Square recurring incident guidance from Pinky: never credit spendable balance from setup intent alone, and when a payment later becomes risky, pause future paid usage until an operator reviews the account.

## What Changed

- Added account-level billing restriction fields:
  - `billing_restricted`
  - `billing_restriction_reason`
  - `billing_restricted_at`
- Migrated existing SQLite databases with additive columns only.
- Added `Account::restrict_billing(...)`, which:
  - marks the account under billing review
  - disables auto-reload
  - clears saved off-session Stripe payment method and Square card metadata
  - preserves customer IDs for evidence and future review
- Added processor-payment lookup through the existing ledger source id in `credit_batches`.
- Added `balance::revoke_processor_credit(...)`, which removes any unspent balance from the refunded/disputed processor payment.
- Added webhook risk handling for Stripe and Square events whose type contains `refund` or `dispute`.
- Added checkout/card-save guards:
  - new payments require verified email
  - restricted accounts cannot add credits or save cards
- Added paid-usage guards:
  - `/router/complete`
  - `/router/complete/stream`
  - `/router/embed`
  - `/router/transcribe`
  - `/stt/session`
- Added auto-reload guard so a stale worker snapshot cannot charge after the account is restricted.

## Important Behavior

- A refund/dispute does not auto-delete the customer or raw webhook evidence.
- A refund/dispute does not attempt to reverse already-consumed credits. It revokes remaining processor-funded balance and blocks future paid usage.
- If a risk webhook cannot be mapped to an account, Bluey logs the failure and returns success to the processor. We do not want Stripe/Square to retry forever on an unmapped event.
- First payment now requires email verification. That gives us a stronger account trail before checkout or card vaulting.
- Counter-dispute filing is still an operator action. The code keeps the evidence trail needed for a rebuttal:
  - raw processor webhook event body
  - processor event id
  - processor payment id
  - account id hash in logs
  - ledger source id
  - usage rows and request ids
  - restriction reason with event id

## Tests Added

- `billing_checkout_requires_verified_email_before_first_payment`
- `billing_square_refund_restricts_account_and_revokes_remaining_credit`
- `billing_stripe_dispute_restricts_account_and_revokes_remaining_credit`
- `router_complete_rejects_billing_restricted_account`

## Verification

```bash
cargo fmt --all --check
git diff --check
cargo test --manifest-path server/Cargo.toml billing_checkout_requires_verified_email_before_first_payment -- --nocapture
cargo test --manifest-path server/Cargo.toml billing_square_refund_restricts_account_and_revokes_remaining_credit -- --nocapture
cargo test --manifest-path server/Cargo.toml billing_stripe_dispute_restricts_account_and_revokes_remaining_credit -- --nocapture
cargo test --manifest-path server/Cargo.toml router_complete_rejects_billing_restricted_account -- --nocapture
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml
```

Observed result: all commands passed. Full server suite passed with 141 unit tests, 1 ConnectInfo integration test, 2 GDPR webhook cleanup tests, and 39 integration e2e tests.

## Areas Most Likely Wrong

- Processor event mapping is necessarily provider-shape-dependent. The tests cover representative Stripe dispute and Square refund events, but real Square dispute payloads should still be replayed from the dashboard before wider alpha.
- Remaining-balance revocation is conservative. It does not reconstruct already-used credits. That is intentional for v0.2 alpha because we care most about stopping further spend immediately.
- Billing restriction recovery is not automated. An operator will need an admin/manual unblock path if a dispute is won or a refund was benign.

## What To Tell Kiro

Please review the billing risk round in:

- `server/src/api/billing.rs`
- `server/src/db/accounts.rs`
- `server/src/db/balance.rs`
- `server/src/api/router.rs`
- `server/src/api/stt.rs`
- `server/src/billing/topup.rs`
- `server/tests/integration_e2e.rs`

Focus on whether the risk webhook mapping is strong enough, whether blocking all paid usage on `billing_restricted` is complete, and whether the evidence trail is enough for counter-dispute operations.
