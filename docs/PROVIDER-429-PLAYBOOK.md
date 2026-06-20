# Provider 429 / Capacity Playbook

> What Bluey does when an upstream provider rate-limits us (429 / 529),
> what to do in code when adding new upstream calls, and the ops levers for
> reducing 429s.
> Author: Kiro · Date: 2026-06-13

This is the canonical reference for upstream capacity handling. If anything
in code contradicts this, it is a bug.

---

## 1. The key fact about provider rate limits

OpenAI and Anthropic enforce rate limits (RPM = requests/min, TPM =
tokens/min) at the **organization/account level, NOT per API key**.

- **Multiple API keys in ONE account do NOT add headroom.** Every key draws
  from the same shared bucket and 429s together. Multiple keys are only
  useful for rotation/revocation/log separation.
- **More headroom within one account comes from:** (a) tier upgrades
  (spend + account-age driven; the intended path), (b) an explicit
  rate-limit-increase request, and (c) **fewer tokens per request** (TPM
  relief — the lever we control in code).
- **Multiple ACCOUNTS** multiply headroom (each has its own bucket) but
  using many accounts specifically to evade limits can violate provider
  ToS. Prefer tier increases; treat multi-account as a documented fallback.

---

## 2. How Bluey survives 429s today (defense-in-depth)

Implemented across `dispatcher.rs`, `provider_health.rs`, `rate_limit.rs`,
and the router candidate loop. New code MUST go through these layers.

1. **Detection** — `dispatcher::retry_after_secs`: a 429 (or 529 = Anthropic
   overloaded) becomes `UpstreamHttpError { retry_after_secs }`, honoring the
   `Retry-After` header or defaulting to `BLUEY_PROVIDER_429_COOLDOWN_SECS`
   (30s).

2. **Key rotation across the pool** — `provider_health::choose_key`: the key
   candidate order is rotated per request (stable hash of
   `request_id:provider:model` in `config::key_candidates_from_pool`), so
   traffic spreads across all keys. A 429'd key is cooled
   (`record_cooldown`) and `choose_key` returns the first non-cooling key.

3. **Provider fallback** — the router candidate loop: if a whole provider is
   cooling (all keys), it falls to the next route (different provider, e.g.
   Anthropic → OpenAI).

4. **Fleet-wide cooldown memory** — `record_cooldown` writes to Redis (local
   fallback), so every server instance routes around a known-throttled key.

5. **Self-throttle to avoid causing 429s** — `rate_limit::check_provider_*`
   per-provider limiters + per-account runaway guardrails.

6. **First-token deadline → fallback** (B2): a provider that accepts (2xx)
   then stalls before the first token falls back to the next route instead
   of hanging.

7. **Clean exhaustion** — when all keys + routes are cooling, the customer
   gets a 503 with `retry_after_secs` (shortest cooldown), so the client
   backs off instead of hammering.

---

## 3. WHAT CODEX MUST DO when adding ANY new upstream provider call

When you add a new managed endpoint or a new provider integration that calls
OpenAI/Anthropic/Deepgram (or any rate-limited upstream), it MUST NOT bypass
the resilience layer. Checklist:

- [ ] Resolve keys through `state.config.upstream.key_candidates(provider, shard_key)`
      where `shard_key` includes the request id (spreads load across the pool).
- [ ] Select the key via `state.provider_health.choose_key(provider, model, &candidates)`,
      and on `Err(CapacityDenied)` fall through to the next route / return a
      503 with `retry_after_secs` (use `capacity_error`).
- [ ] Pre-check `state.rate_limiters.check_provider_*(provider, model)` (and
      the per-account limiter) before dispatch.
- [ ] On an upstream error, call `routing::upstream_retry_after(&err)`; if it
      returns `Some(secs)`, call
      `state.provider_health.record_cooldown(provider, model, key_fingerprint, secs)`
      and try the next route — do NOT surface a raw 429 to the customer.
- [ ] Iterate `resolve_route_candidates(lane)` (multi-provider) rather than a
      single hard-coded provider, so a cooled provider can be skipped.
- [ ] Bound token usage: respect `effective_max_output_tokens` (which now
      clamps non-thinking lanes) and keep input context bounded.
- [ ] On exhaustion return a 503 + `retry_after_secs`, never a hang.

If you copy an existing handler (`complete`, `complete_stream`, `embed`,
`transcribe`) you inherit all of this. If you write a bespoke upstream call,
route it through the same helpers.

---

## 4. Reducing the RATE of 429s (not just surviving them)

In priority order:

1. **Request a tier increase / higher usage limit** on the provider account.
   This is the single biggest lever and is ops, not code. Fund + use the
   account consistently; tiers auto-promote.
2. **Token-per-request relief (code, partly done):**
   - `BLUEY_MAX_OUTPUT_TOKENS` clamps non-thinking output budgets (default
     2048) — bounds TPM reservation per request.
   - B1: RAG context is retrieved under a budget and capped in size.
   - Transcript context is bounded (~10 recent segments).
   - Keep prompts tight; prefer the instant lane for short answers.
3. **Add keys from ADDITIONAL accounts** to the `*_API_KEYS` pool (comma-
   separated) — multiplies headroom, but mind provider ToS (see §1).
4. **Weighted key selection** (future): if accounts are on different tiers,
   spread traffic proportionally rather than evenly. Not implemented.

---

## 5. Ops env reference

| Env | Purpose | Default |
|---|---|---|
| `OPENAI_API_KEYS` / `ANTHROPIC_API_KEYS` / `GEMINI_API_KEYS` / `GOOGLE_API_KEYS` / `DEEPGRAM_API_KEYS` | comma-separated key pools (fall back to singular `*_API_KEY`) | - |
| `BLUEY_PROVIDER_429_COOLDOWN_SECS` | cooldown when no Retry-After header | 30 |
| `BLUEY_MAX_OUTPUT_TOKENS` | non-thinking output ceiling (TPM/cost) | 2048 |
| `BLUEY_STREAM_FIRST_TOKEN_TIMEOUT_MS` | stall → fallback deadline (B2) | 6000 |
| `BLUEY_RAG_RETRIEVAL_BUDGET_MS` | RAG retrieval budget (B1) | 300 |
| `BLUEY_UPSTREAM_SPEND_LIMIT_CENTS` / `_WINDOW_HOURS` | global spend circuit breaker | off |

Redis (if configured) makes cooldowns + capacity limits fleet-wide; without
it they are per-instance (local fallback).

---

## 6. Customer-facing behavior on capacity exhaustion

When Bluey genuinely can't serve (all keys + routes cooling), the API returns
**503 with `retry_after_secs` + `reason: "upstream_spend_guard"` /
`"provider_key_cooling_down"`**. The desktop/overlay should show a brief
"capacity is busy, retrying shortly" state and honor the retry-after rather
than spamming. (Overlay copy is codex's lane — flagging for that polish.)
