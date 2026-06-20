# Square Reload Fraud Invariants — 2026-06-20

## Trigger

Bluey is moving toward real paid testing with Square reloads and opt-in Auto
Reload. The remaining abuse concern was a mismatch between what Square says was
paid and what Bluey credits into the account ledger.

## Invariant

Bluey must never create spendable balance unless all of these match:

- Square webhook signature is valid for the active billing environment.
- Square order/payment is completed.
- Bluey account metadata and Square reload reference resolve to the same account.
- Square currency is `USD`.
- Square paid amount equals Bluey's expected reload amount.
- Credited amount equals the verified Square amount.
- The credit batch points to exactly one Square payment id.

If any of those fail, the event is suspicious or misconfigured. Bluey fails
closed, creates no credit batch, and leaves the event for operator review.

## Code Changes

- `server/src/api/billing.rs`
  - Square order webhooks now require explicit Bluey account metadata,
    Square reload reference, expected amount metadata, USD total, matching
    optional line/tender amounts, and exactly one tender payment id.
  - Square payment webhooks now require a Bluey reload reference, USD amount,
    minimum reload amount, optional metadata consistency, and a payment id.
  - Order id is no longer used as a fallback payment id for crediting.

- `server/src/billing/topup.rs`
  - Square Auto Reload validates the returned Square payment before crediting:
    amount, currency, reference id, and customer id must match the charge Bluey
    initiated.

- `server/tests/integration_e2e.rs`
  - Added regression coverage that amount mismatches do not credit balance.
  - Added regression coverage that account metadata/reference mismatches do not
    credit either account.
  - Updated the Auto Reload mocked Square payment response to include the fields
    now required before crediting.

- `docs/deploy/SQUARE-BILLING.md`
  - Documented the Square reload invariant and fail-closed behavior.

- `docs/deploy/ABUSE-FRAUD-CHARGEBACK-PLAYBOOK.md`
  - Added reload mismatch as an abuse/fraud scenario and launch checklist item.

## Still Required Before Wider Paid Alpha

- Run one Square sandbox reload success and replay the webhook.
- Run one sandbox mismatch replay and confirm the deployed server fails closed.
- Run one low-dollar production reload success.
- Confirm Square failed-webhook/dispute emails go to a monitored inbox.

## Verification

- `cargo fmt --all --check` ✅
- `git diff --check` ✅
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e billing_square_webhook -- --nocapture` ✅
  - valid Square completed order credits balance
  - production-signed event is rejected while checkout is sandbox
  - amount mismatch fails closed and credits nothing
  - account metadata/reference mismatch fails closed and credits neither account
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e square_auto_reload_charges_saved_card_when_threshold_crosses -- --nocapture` ✅
  - saved-card Square Auto Reload sends one Square payment request and credits
    only after the completed payment response passes amount/reference/customer
    validation
- `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings` ✅
- `cargo test --manifest-path server/Cargo.toml` ✅
  - 141 lib tests
  - 35 integration e2e tests
  - ConnectInfo/GDPR suites

Implementation note: the Auto Reload regression exposed a real bug where the
Square payment response was already narrowed to `payment`, but the code still
looked for `/payment/id`. That is fixed to `/id` in the same round.

## Areas Most Likely Wrong

- Square hosted-checkout `payment.updated` events may arrive before
  `order.updated`. Payment events are still accepted when signed, completed,
  Bluey-referenced, USD, and above the reload minimum; the stricter expected
  amount metadata is enforced on order events and Auto Reload responses.
- The existing table name `stripe_webhook_events` remains in use for Square
  webhook idempotency with a `square:` prefix. This is intentionally deferred
  until a lower-risk schema cleanup.
