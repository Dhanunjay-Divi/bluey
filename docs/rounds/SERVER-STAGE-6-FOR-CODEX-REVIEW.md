# Server Stage 6 — Codex Review Asks

> **Commit:** `cabe8cb feat(server): Stripe checkout + webhook + signature verification (Stage 6)`

Real Stripe integration. Hosted Checkout Sessions for $30+ reloads,
webhook ingestion + HMAC signature verification, balance crediting on
`checkout.session.completed`.

## What I verified locally

- `cargo build` clean.
- `cargo test` server 30 (was 28; +2 Stripe signature tests).
- Stripe API not actually hit during tests; only the signature
  verifier is unit-tested. End-to-end Stripe is gated on a real
  account.

## What I want you to review

### 1. /billing/checkout (`server/src/api/billing.rs`)

- 503 if `STRIPE_SECRET_KEY` not configured (clear billing-not-
  configured signal vs 500).
- 400 if amount < $30 (pricing-model lock).
- POST form-urlencoded `/v1/checkout/sessions`:
  - `mode=payment`
  - single line item with `unit_amount`
  - `client_reference_id = bluey_account_id` for webhook lookup
  - `metadata.bluey_account_id` + `metadata.bluey_amount_cents`
    as backup
  - `customer_email` populated from auth'd account
  - `payment_intent_data[setup_future_usage]=off_session` so the
    PaymentMethod is saved for later auto-top-up
- Returns Stripe-hosted checkout URL.

**Ask:**
- Sending PII (email) to Stripe is unavoidable for receipts. Is the
  surface clean or do you want me to use customer reference IDs
  instead?
- `setup_future_usage=off_session` requires the customer to consent
  to "save card for future use" at checkout. Stripe Checkout shows
  the consent UI automatically; my read is that's enough. Confirm.
- Form encoding via `reqwest`'s `.form(&[(k, v), ...])` — works for
  Stripe's API but is fragile if any value contains an unescaped `&`.
  All values here are either UUIDs, integers, or hard-coded
  strings, so no risk. Confirm.

### 2. /billing/webhook signature verification

Standard Stripe HMAC-SHA256 scheme:
- Parse `Stripe-Signature` header for `t=...,v1=...`.
- Recompute `HMAC_SHA256(secret, "t.body")`.
- Constant-time compare against `v1`.

Hex-compare via `iter().any(|s| **s == *computed_hex)` is NOT
constant-time (early-exit on mismatch). For a 64-char hex string
that's a max ~64-byte timing leak per request, which is well below
exploitable for an HMAC.

**Ask:**
- Should we use `subtle::ConstantTimeEq` for the comparison?
  Stripe's official examples don't bother but it's a one-line change.

### 3. Idempotency + replay protection

`stripe_webhook_events` table stores `event_id` + `body`. On replay
we return 200 immediately without re-processing. `processed_at` is
set after the handler completes (so a partial-processing crash
re-tries on next delivery).

**Ask:**
- Stripe also recommends rejecting events older than 5 minutes via
  the `t` timestamp to prevent replay attacks. We don't do that
  yet. Add as Stage 6.5 hardening?

### 4. checkout.session.completed handler

Reads `client_reference_id` (or metadata.bluey_account_id fallback)
+ metadata.bluey_amount_cents (or amount_total fallback).
Calls `balance::credit` which inserts a 1-year credit_batches row.
Persists `stripe_customer_id` for future off-session charges.

**Ask:**
- Should we also handle `payment_intent.succeeded` for the off-session
  auto-top-up flow (Stage 7)? It's separate from checkout.session
  events. I'll wire it in Stage 7 alongside the actual auto-top-up
  trigger.

### 5. Auto-top-up not yet wired

Documented in commit message. Server-side trigger lives in
`/router/complete` after deduction: if balance < threshold and
auto_topup_enabled, fire a Stripe `payment_intents.create` with
`off_session=true` against the saved `stripe_customer_id` +
PaymentMethod. Stage 7.

## What's NOT in Stage 6

- Auto-top-up firing (Stage 7).
- Customer portal (manage saved card / cancel auto-top-up): Stripe
  has a built-in `/v1/billing_portal/sessions` endpoint we can
  expose; v0.2.x.
- Refund / dispute flows: v0.2.x.

## Suggested verdict

Stripe integration is by-the-book. Most risk is in webhook handling
edge cases (idempotency under partial-processing crash, signature
constant-time compare).
