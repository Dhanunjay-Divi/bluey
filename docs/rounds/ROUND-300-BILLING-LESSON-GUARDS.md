# Round 300 - Billing Lesson Guards

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The user provided the Pinky billing lesson pack and asked Bluey to avoid the
same production billing mistakes:

- do not trust checkout redirects as payment success
- do not unlock access from loose provider state
- revoke usage quickly on refunds/disputes
- block admin/test accounts from real paid checkout
- keep a real immutable ledger and reconciliation evidence

## Existing Bluey State

Bluey is a credit/reload product, not a recurring subscription product, so the
main Pinky subscription lesson maps to processor-confirmed reloads and strict
credit ledger behavior.

Already present before this round:

- Stripe and Square webhooks are verified before processor credits are added.
- Reload credits go through `credit_processor_payment`, keyed by provider
  payment id for idempotency.
- Refund/dispute events revoke remaining processor-backed credit where possible.
- Refund/dispute events mark the account billing-restricted and disable Auto
  Reload.
- Router, STT, sync/RAG, and usage ingestion reject billing-restricted accounts.
- Balance movement rows include before/after balance, reason, source/provider
  identifiers, idempotency key, request id, and metadata.

## Fix

Added a Pinky-style guard for Bluey internal/admin/test accounts:

- admin accounts cannot start paid checkout or save cards
- `internal-*`, `test-*`, and `admin-test-*` `@bluey.sh` accounts cannot start
  paid checkout or save cards
- obvious test emails such as `+test` and `@test.local` cannot start paid
  checkout or save cards
- internal/admin/test accounts cannot enable Auto Reload
- `/account/me` now reports Auto Reload unavailable for these accounts so the UI
  should not invite a real card/payment setup path

This specifically covers the active internal account pattern:

```text
internal-admin-20260606023943@bluey.sh
```

## Verification

Added a billing unit test covering:

- internal admin Bluey account
- admin flag
- plus-test email
- test.local email
- normal customer email

Verification passed:

```bash
cargo test --manifest-path server/Cargo.toml internal_and_test_accounts_cannot_enter_paid_billing_flows -- --nocapture
cargo test --manifest-path server/Cargo.toml billing -- --nocapture
cargo check --manifest-path server/Cargo.toml --quiet
git diff --check
```

## Current State

Internal/test accounts should receive credits only through internal admin/test
credit grants, not real checkout, card-save, or Auto Reload flows. Real users
can still use normal reload/payment flows if their account is not billing
restricted and their email is verified.

## Remaining Gates

- Add a polished reconciliation dashboard showing local ledger, processor
  payment state, refund/dispute state, and unmapped provider payments.
- Store explicit checkout/reload consent evidence: terms version, IP, user
  agent, selected amount, threshold, and saved-payment consent.
- Add durable Auto Reload attempt rows with idempotency, monthly spend guard,
  receipt state, provider ids, and failure reason.
- Add a manual owner-reviewed reinstate path for won disputes or benign refunds.
- Include balance ledger evidence in account export/support bundles.
