# REVIEW: Server Stage 4 — Router Complete, Pricing, FIFO Credits

**Commit:** `e0a77dc feat(server): real /router/complete + FIFO consumption + pricing fix (Stage 4)`  
**Reviewer:** Codex  
**Date:** 2026-05-19

## Per-Task Review

### Stage 4 — Money Path

| Field | Value |
|-------|-------|
| Files | `server/src/api/router.rs`, `server/src/db/balance.rs`, `server/src/pricing/mod.rs`, `server/src/routing/dispatcher.rs` |
| Verdict | 🔴 blocker |

**Findings:**

- 🔴 `server/src/api/router.rs:13` / `server/src/api/router.rs:171` — `/router/complete` has no client-provided idempotency key, then generates a fresh `request_id` after the upstream call. If the server completes the upstream request and deduction but the client retries after a timeout/lost response, the retry will dispatch and charge again. This is the primary production billing blocker. Add `request_id`/idempotency key to `CompleteRequest`, persist request state under `(account_id, request_id)`, and return the cached result or an in-progress conflict on retries.
- 🔴 `server/src/db/balance.rs:121` — trial accounting floors `ms / 1000`, so every sub-second successful request decrements zero seconds. On the instant lane this can turn the 10-minute trial into effectively unlimited fast calls. Charge a minimum of 1 second for every successful trial request, or move trial accounting to wall-clock active-session time as a separate server-side lease.
- 🔴 `server/src/pricing/mod.rs:83` and `docs/PRICING-MODEL.md:55` — implementation rounds every request up to whole customer cents, but the customer-facing pricing doc and CLI examples still advertise fractional-cent easy cues like `$0.0003`. With current code, easy cues are `1c`, not `0.03c`, changing the tier projections materially. Either store/deduct in microcents or update all customer-facing pricing/tier math to reflect the 1-cent minimum.
- 🔴 `server/src/routing/dispatcher.rs:24` / `server/src/routing/dispatcher.rs:43` — `resolve_route("local")` maps to `ollama/llama3.1`, and pricing contains `ollama`, but `complete()` rejects every provider except OpenAI and Anthropic. This makes the advertised local lane return a 502. If local is daemon-only, the server should reject `local` with a clear 400 and managed routing should not send local-only work to the cloud.
- 🟡 `server/src/routing/dispatcher.rs:137` — if OpenAI ever omits `usage`, Stage 4 charges `(0, 0)` instead of falling back to the entry estimate. Rare, but for a billing proxy the safer fallback is to charge the estimate or mark the request for reconciliation.
- 🟡 `server/src/api/router.rs:119` / `server/src/routing/dispatcher.rs:126` — upstream non-2xx bodies are surfaced through `ApiError.error`. This is useful in dev but can expose provider internals to customers; production should return a sanitized message and keep raw upstream details in logs.

## Cross-Task Findings

- The hard-stop overrun behavior is directionally right, but it needs an idempotency layer before it is safe under retries and concurrent requests.

## Build & Test Verification

```bash
cd server && cargo test --lib   # ✅ 30 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved before this stage can be treated as production money-path code.

## Follow-ups for Next Batch

- Add a focused integration test for retry/idempotency around `/router/complete`.
- Decide whether Bluey bills in whole cents or microcents, then make docs, CLI, and server consistent.
