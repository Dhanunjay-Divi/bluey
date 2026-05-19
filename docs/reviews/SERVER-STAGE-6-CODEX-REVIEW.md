# REVIEW: Server Stage 6 — Stripe Checkout and Webhook

**Commit:** `cabe8cb feat(server): Stripe checkout + webhook + signature verification (Stage 6)`  
**Reviewer:** Codex  
**Date:** 2026-05-19

## Per-Task Review

### Stage 6 — Billing Ingress

| Field | Value |
|-------|-------|
| Files | `server/src/api/billing.rs`, `server/src/db/balance.rs`, `server/src/api/mod.rs`, `server/src/api/admin.rs` |
| Verdict | 🔴 blocker |

**Findings:**

- 🔴 `server/src/api/billing.rs:191` / `server/src/api/billing.rs:203` — webhook idempotency is not atomic with crediting. The event row is inserted, `handle_checkout_completed()` credits the account, and then `processed_at` is updated with the result ignored. If the process crashes or the final update fails after crediting, the next webhook retry sees `processed_at = NULL` and credits again. Wrap event insert, credit, and processed mark in one transaction, or add a unique payment/session id guard that makes `balance::credit` idempotent.
- 🔴 `server/src/db/balance.rs:67` — `balance::credit()` inserts a `credit_batches` row and updates `accounts.balance_cents` in two independent statements outside a transaction. A failure between them leaves batch and balance divergent. This is especially risky now that Stripe webhooks call it directly. Make credit atomic.
- 🔴 `server/src/api/billing.rs:245` — Stage 6 claims the PaymentMethod is saved for future auto-top-up, but the handler only persists `stripe_customer_id`; `stripe_payment_method_id` is never populated. Auto top-up cannot reliably charge off-session from this state. Persist the payment method from the Checkout/PaymentIntent path or explicitly defer auto top-up data capture.
- 🔴 `server/src/api/mod.rs:61` / `server/src/api/admin.rs:33` — `/admin/customers` is only protected by normal bearer auth and the handler does not require `account.is_admin`. Any authenticated customer can list customer ids, emails, and balances. This predates the Stripe work, but Stage 6 touched the API surface and it blocks production billing rollout.
- 🟡 `server/src/api/billing.rs:262` — Stripe signature verification parses `t=` but never enforces a timestamp tolerance, and compares signatures with normal string equality. Stripe’s webhook docs recommend using a tolerance to reduce replay risk; use a constant-time compare as well.
- 🟡 `server/src/api/billing.rs:103` — checkout errors return raw Stripe response bodies to the caller. Sanitize customer-facing errors and keep provider details in server logs.

## Cross-Task Findings

- Billing should get a small ledger/idempotency test suite before the next money-path stage: duplicate webhook, crash-before-processed simulation, duplicate payment intent/session, and credit transaction failure.

## Build & Test Verification

```bash
cd server && cargo test --lib   # ✅ 30 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Do not ship payment crediting until webhook crediting is atomic/idempotent and admin access is role-gated.

## Follow-ups for Next Batch

- Add `require_admin` middleware or an explicit `AuthedAccount` check in admin handlers.
- Add Stripe webhook timestamp tolerance and constant-time signature comparison.
