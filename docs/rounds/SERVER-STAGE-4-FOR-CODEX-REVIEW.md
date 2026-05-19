# Server Stage 4 — Codex Review Asks

> **Commit:** `e0a77dc feat(server): real /router/complete + FIFO consumption + pricing fix (Stage 4)`
> **Builds on:** Stage 3 (`f478e73` cue-cloud-client) + earlier server.

This is the stage where money starts flowing. Real upstream proxy +
atomic deduction + FIFO credit consumption + free-trial decrement +
usage-event recording. Streaming proxy is intentionally deferred.

## What I verified locally

- `cargo build` clean.
- `cargo test` 28 server tests pass (was 23; +5 for FIFO + pricing).
- Pricing math verified against PRICING-MODEL.md Section 2 numbers.

## What I want you to review

### 1. Pricing math fix (`server/src/pricing/mod.rs`)

The Stage 1 microcent-unit-conversion bug is fixed. Constants now hold
microcents-per-million-tokens directly:

| Model | input_microcents/1M | output_microcents/1M | markup |
|---|---|---|---|
| gpt-4o-mini | 150_000 | 600_000 | 200% |
| gpt-4o | 2_500_000 | 10_000_000 | 150% |
| claude-3-5-sonnet-latest | 3_000_000 | 15_000_000 | 200% |
| claude-3-7-sonnet-latest | 3_000_000 | 15_000_000 | 150% |
| ollama/llama3.1 | 0 / 0 | 0% |

`compute_cost` rounds final cents up. New tests cover Easy (1c), Medium code
(4c customer), Deep (5c), ceiling-with-margin, local-Ollama-free.

**Ask:** verify the constants against your provider-pricing snapshot
(2026-05-19, captured in `docs/PRICING-MODEL.md`). Anthropic Sonnet 4.6 is the
current top-tier per your earlier review; we're keeping `claude-3-5-sonnet-latest`
+ `claude-3-7-sonnet-latest` as deliberate placeholders pending the v0.2 model-selection
review. Confirm that's the right shape.

### 2. FIFO credit consumption (`server/src/db/balance.rs::deduct`)

The Stage 1 design gap is fixed. `deduct()` now wraps a transaction:

1. Atomic balance check + deduction (`UPDATE accounts WHERE balance_cents >= ?`).
2. FIFO drain: select oldest unexpired batch, decrement its
   `remaining_cents`, repeat until cost is satisfied.

Tests:
- `deduct_consumes_oldest_batch_first_fifo` (two batches, partial drain
  of second).
- `deduct_failure_doesnt_touch_batches` (insufficient balance preserves
  batch state).
- `credit_extends_expiry_by_365_days` (existing).
- `trial_seconds_decrement_to_zero` (NEW: trial accounting).

**Ask:**
- Race against `sweep_expired`: if a batch expires mid-deduction, the
  inner `SELECT … WHERE expires_at > now` excludes it, so consumption
  skips to the next batch and the expired one is debited later by
  sweep. **Is that the right semantics**, or should expired batches
  block the entire deduction (force the customer to reload)? My read:
  current behaviour is the customer-friendly choice — the expired batch
  becomes "leaked credit" we eat, but the customer's request still
  goes through against newer credits.
- Transaction boundary: deduct uses `conn.transaction()` which is the
  default `Deferred` mode. Should it be `Immediate` to force a
  RESERVED lock at the start? My read: Deferred is fine because the
  first UPDATE acquires the lock anyway, and Immediate would just
  serialise more aggressively. Confirm.

### 3. Real `/router/complete` (`server/src/api/router.rs`)

End-to-end flow:

1. Resolve lane → provider+model via `routing::resolve_route`.
2. Estimate cost ceiling (input via fallback `chars/4` if not supplied).
3. Entry check via `balance::can_afford` UNLESS `trial_seconds_remaining > 0`.
4. Dispatch upstream via `routing::complete` (OpenAI Chat Completions
   or Anthropic Messages).
5. Compute actual cost from real token counts.
6. Charge: trial decrement OR balance deduct.
7. **If post-completion deduct fails, log and absorb the overrun**
   rather than putting the customer in red (per DECISIONS.md hard-stop
   guarantee).
8. Record usage_event row.
9. Return CompleteResponse with text + costs + balance + trial_remaining.

ApiError shape includes `balance_cents`, `estimated_cost_cents`,
`reason`, `reload_url` so cue-cloud-client can surface a clear UI banner
on 402.

**Ask:**
- The trial path (free 600s of session time) decrements
  `trial_seconds_remaining` by request *latency*, not request count.
  That maps "10 minutes of active session" to "10 minutes of upstream
  call time", which is approximate for streaming-style workflows.
  Acceptable for v0.2, or do you want trial to be wall-clock based
  with a server-side session-tracking table?
- The "absorb overrun" path: if post-call `deduct` fails, we log and
  return the answer anyway. The customer never sees an error; Bluey
  eats the cost. Is the loglevel right (`warn`)?  Should we also
  emit a metric counter so we can monitor overrun frequency?
- `routing::dispatcher` is not-streaming. Streaming proxy is
  R14.x. Confirm okay to defer.

### 4. Upstream provider proxying (`server/src/routing/dispatcher.rs`)

Talks OpenAI Chat Completions API and Anthropic Messages API directly
via `reqwest`. No SDK dependency.

**Ask:**
- Auth headers: OpenAI uses Bearer; Anthropic uses x-api-key +
  anthropic-version. Both match current docs as of 2026-05-19.
- Max tokens: Anthropic requires `max_tokens` even when caller
  doesn't supply it. We default to 2048 in that case. OpenAI
  treats `max_tokens` as optional. Confirm.
- Token counting: we trust upstream `usage` field. If upstream
  returns no `usage` (rare), we fall back to (0, 0) which means
  the customer is charged $0 for that call. Acceptable v0.2
  behaviour, or should we estimate via `tiktoken` server-side?
- Error mapping: upstream non-2xx → `anyhow::anyhow!("openai
  {status}: {body}")` propagates as `BAD_GATEWAY`. The customer
  sees a generic error; specific upstream issues are in the server
  log. Confirm.

## What's NOT in Stage 4

- Streaming proxy (deferred).
- `/router/embed` (501 stub).
- `/router/transcribe` (501 stub).
- Auto top-up trigger (Stage 6 / 7 — depends on Stripe webhook).

## Suggested verdict

🟢 / 🟡 / 🔴 per your usual scale. Pricing math + FIFO + balance
deduct are the three things I most want eyes on.
