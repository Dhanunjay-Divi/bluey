# Round 039 - Paid Chat Observability — 2026-06-10

## Goal

Make the live-currency test debuggable from the first reload through the first paid chat without exposing secrets, prompt text, checkout URLs, or raw account IDs.

## What changed

- `/billing/checkout` now logs `billing checkout requested` and `billing checkout created`.
- Stripe and Square credit webhooks now log only `account_id_hash`, never raw `account_id`.
- `/router/complete` and `/router/complete/stream` now log the paid chat lifecycle:
  - `managed chat request accepted`
  - `managed chat idempotency reserved`
  - `managed chat idempotency replayed completed response`
  - `managed chat duplicate request still in progress`
  - `managed chat duplicate request previously failed terminally`
  - `managed chat memory context prepared`
  - `managed chat route selected`
  - `managed chat usage event recorded`
  - `managed chat usage event deduplicated`
  - `managed chat completed and billed`

## Fields To Grep During Live Tests

- `trace_id`: HTTP boundary trace, returned in `X-Bluey-Trace-Id`.
- `request_id`: stable per chat submission, idempotency key, and usage-event dedupe key.
- `session_id`: local/cloud session identifier when the desktop sends it.
- `account_id_hash`: hashed account prefix for customer correlation without logging raw account IDs.
- `billing_provider`: `square` or `stripe`.
- `provider` and `model`: selected upstream model after routing/fallback.
- `lane` and `effective_lane`: requested route and actual route, including vision override.
- `rag_match_count`: how much saved memory/docs were attached to the chat.
- `cost_cents`, `balance_cents_after`, `trial_seconds_remaining`: billing result.
- `streaming`: distinguishes `/router/complete/stream` from single-shot `/router/complete`.

## Redaction Contract

These logs must not contain:

- Provider API keys or authorization headers.
- User prompt text, transcript text, attached document text, or model response text.
- Checkout URLs.
- Raw account IDs or email addresses.

## Tomorrow's Test Path

1. Start server with normal production-like logging.
2. Add credits from the account page.
3. Confirm `billing checkout requested` and `billing checkout created`.
4. Complete Square/Stripe payment.
5. Confirm `credited from Square webhook` or `credited from Stripe webhook`.
6. Submit one typed chat from the desktop overlay.
7. Grep by the desktop `request_id` and confirm the lifecycle reaches `managed chat completed and billed`.
8. Confirm a matching `managed chat usage event recorded` log exists.
9. Confirm balance changed in the account page and overlay.

