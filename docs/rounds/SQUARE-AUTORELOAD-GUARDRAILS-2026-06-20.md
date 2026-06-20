# Square Auto-Reload Guardrails — 2026-06-20

## Trigger

Pinky found an incident-class billing gap after moving from Stripe subscriptions to Square: the current Square customer setup did not create recurring payment behavior, so any product assuming a saved recurring mandate could leave auto-reload broken or unsafe.

Bluey must not repeat that mistake.

## Product Invariant

Bluey credits are ledger-backed account credits. A spendable credit batch may be created only from:

1. A processor-confirmed successful payment, with a stable payment id.
2. An explicit internal/operator credit source.

Starting checkout, creating a payment link, saving a card, creating a setup intent, or receiving a non-completed webhook must never create spendable balance.

## Code Changes

- `server/src/db/balance.rs`
  - Removed the public optional-source credit footgun.
  - Added `credit_processor_payment(provider, processor_payment_id)` for webhook crediting.
  - Added `credit_internal(reason)` for tests/operator-style explicit grants.
  - Empty provider/payment ids are rejected before the ledger is touched.

- `server/src/api/billing.rs`
  - Stripe compatibility webhook now refuses to credit `checkout.session.completed` without a `payment_intent`.
  - Square webhook credits through `credit_processor_payment("square", payment_id)`.

- `server/src/billing/topup.rs`
  - Legacy Stripe auto-topup is now gated by `BillingProvider::Stripe`.
  - If Square is active, old Stripe customer/payment-method metadata cannot fire a background charge.

- `server/src/db/accounts.rs` and `server/src/db/mod.rs`
  - New accounts default to manual reload (`auto_topup_enabled = false`).
  - Added Square customer/card-on-file metadata. Only Square customer/card ids and display-safe card brand/last4 are stored; Bluey never stores raw card numbers, CVV, or expiration.

- `server/src/api/account.rs`
  - Added `PATCH /account/billing` so the account dashboard can turn Auto Reload on/off.
  - Enabling is rejected until the active billing provider has a saved off-session payment method.
  - Reload amount and threshold are validated against the Bluey Auto Reload spec: default threshold `$5`, minimum reload `$15`, and reload amount must be greater than the threshold.

- `server/src/api/billing.rs`
  - Added `POST /billing/square/card` to save a Square Web Payments card token through Square's Cards API.
  - The endpoint creates/reuses a Square customer and saves only the returned Square card id plus display metadata.

- `server/tests/integration_e2e.rs`
  - Added coverage that Square mode does not run legacy Stripe auto-topup even when stale Stripe metadata exists.
  - Added coverage that Square Auto Reload cannot be enabled before a card is saved.
  - Added coverage that crossing the Auto Reload threshold charges the saved Square card and credits only after a completed Square payment.

- `web/index.html`, `web/assets/bluey-site.js`, and `web/assets/bluey-site.css`
  - Added an account-dashboard Auto Reload toggle.
  - New accounts show Auto Reload off by default.
  - Users can choose the threshold and reload amount before enabling Auto Reload.
  - Square card setup appears only when needed and uses Square-hosted card entry.

- `ops/Caddyfile.example`
  - Allows the Square Web Payments SDK domains and routes `/account/billing` through the API.

- `docs/PRELAUNCH-CHECKLIST.md`
  - Added Square duplicate/failure live checks.
  - Marked code guardrails closed and left live sandbox/production reload proof open.

## Still Required Before Wider Paid Alpha

- Complete Square sandbox checkout and confirm balance credits within 30 seconds.
- Replay the same Square webhook and confirm no double credit.
- Send/observe a failed or canceled Square payment event and confirm no credit.
- Complete one low-dollar production Square reload and confirm the production webhook credits once.
- Confirm Square dispute/failed-webhook notices route to a monitored operator inbox.

## Verification

- `cargo fmt --all --check`
- `git diff --check`
- `cargo test --manifest-path server/Cargo.toml db::balance::tests`
- `cargo test --manifest-path server/Cargo.toml stt_accounting`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e auto_topup_off_by_default_does_not_fire_charge`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e square_mode_never_runs_legacy_stripe_auto_topup`
- `cargo test --manifest-path server/Cargo.toml --test integration_e2e square_auto_reload`
- `cargo test --manifest-path server/Cargo.toml --lib`
- `cargo test --manifest-path server/Cargo.toml`
- `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`

## Areas Most Likely Wrong

- The database column remains named `stripe_charge_id` for compatibility even though it now stores namespaced ids like `square:payment_123`. This is intentionally deferred to avoid a risky migration close to live testing.
- Existing production rows keep their current `auto_topup_enabled` values. Square deployments are protected by the provider gate, but a future Stripe-mode deployment must review legacy Stripe card state before enabling automatic top-up.
- Square card-on-file still needs a live sandbox card-save smoke in the browser because the unit/integration tests use mocked Square responses.
