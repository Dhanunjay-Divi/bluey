# Billing Credits + Policy Alignment — 2026-06-01

## User Decision

Bluey is **not** a monthly-subscription-first product. The v0.2 alpha product
uses reloadable account credits:

- Customers add credits through hosted checkout.
- Credits are non-transferable, have no cash value, and are not a wallet,
  stored-value card, or gift card.
- Managed usage spends credits per request.
- Credits stop at zero so customers do not accrue usage debt.
- Credit batches are valid for up to 12 months, implemented as 365 days from
  purchase.

## Implementation Verified

`server/src/db/balance.rs` already had the correct ledger behavior:

- `credit()` creates a credit batch with `expires_at = now + 365 days`.
- `deduct()` spends FIFO from the oldest unexpired batch.
- `sweep_expired()` zeros remaining expired credits and marks the batch
  expired.
- Tests cover FIFO consumption, idempotent crediting, and expiry window.

This pass added `CREDIT_VALIDITY_DAYS` so the 365-day policy is explicit and
not a magic number.

## Copy And Documentation Updated

- `web/index.html`
  - Replaced stale "wallet" wording in the account preview.
  - Updated credit expiry copy to "up to 12 months (365 days)".
  - Added `/docs/privacy` and `/docs/terms` routes inside the static site.
  - Linked account sign-in/create-account copy to Terms and Privacy.

- `DECISIONS.md`, `ARCHITECTURE.md`, `docs/HOW-IT-WORKS.md`,
  `docs/PRICING-MODEL.md`, `docs/deploy/SQUARE-BILLING.md`
  - Reframed billing as account credits/manual reload, not monthly
    subscription, wallet, or live auto-topup.

- `docs/PRELAUNCH-CHECKLIST.md`
  - Marked alpha Terms/Privacy pages as published.
  - Kept final legal review as required before broad launch.

## Remaining Follow-Ups

- Final counsel/product review of Terms and Privacy before public launch.
- Refund/support policy page or account help copy.
- Per-batch credit expiry display in `bluey credits` and `/account`.
- Email warning before a credit batch expires.
- Saved-card auto-reload only after Square saved-card flow is implemented and
  reviewed; do not market it as live until then.
