# Codex Fix Wave Round 2 — Stages 2 + 4-8 — Re-Review Asks

> **Branch:** `feat/phase-3-round-12`
> **Tip:** `0a3829e fix(cli): Stage 8 login keyring bridge + soften credits help`
> **Replaces:** Codex review verdicts 🔴 on Stages 2, 4, 5, 6, 7, 8.

This wave clears every blocker from the second-round codex reviews
filed in `docs/reviews/SERVER-STAGE-{2,4,5,6,7,8}-CODEX-REVIEW.md`.

## Pipeline at the tip

```
✅ cargo fmt --all (cue + server)
✅ cargo clippy --all-targets -- -D warnings (cue + server)
✅ 402 cue cargo tests + 44 server cargo tests + 15 vitest
✅ builds (debug, all crates)
```

Test growth across the wave:

| Crate | Before | After | Delta |
|---|---|---|---|
| server | 30 | 44 | +14 (+3 admin, +5 idempotency, +1 trial-min, +2 credit, +1 stale-sig, +2 usage) |
| cue-router | 30 | 30 | net zero (replaced one local-only test with stronger never-emits-local) |
| cue-llm | 32 | 34 | +2 (Billing failover) |

## Per-stage closure

### Stage 2 (recheck) — 🟢 cleared in `5743465`

| Codex blocker | Fix |
|---|---|
| S2.2 `/admin/customers` admin-role gating | New `require_admin` middleware layered AFTER `require_auth`. Returns 401 if no AuthedAccount, 403 if `!is_admin`. 3 new tests using `tower::ServiceExt::oneshot`: unauthenticated → 401, non-admin → 403, admin → 200. |

### Stage 4 — 🟢 cleared in `f965d91`

| Codex blocker | Fix |
|---|---|
| S4.1 `/router/complete` retry double-charge | New `request_idempotency` table (migration 0007) + `db/idempotency.rs` module with `reserve` / `mark_complete` / `mark_failed` / `release` helpers. `CompleteRequest` now requires client-supplied `request_id`. Replays return cached response (200) or 409 in-progress / failed-terminal. 5 new tests. |
| S4.2 trial sub-second exploit | `consume_trial_seconds` ceiling-divides ms→s with a 1-second floor. New test asserts a sub-second request consumes ≥1s of trial budget. |
| S4.3 pricing fractional vs whole-cent mismatch | PRICING-MODEL.md Section 2 has an explicit "Billing precision" note documenting whole-cent rounding (1¢ minimum). Tier projections updated 2,850 → 3,000 cues/$30 Light. CLI `bluey usage` table updated to match. |
| S4.4 `local` lane misroutes | `/router/complete` rejects `lane=local` with 400 + `reason=local_lane_unsupported`. dispatcher rejects unsupported providers (incl. `ollama`) with a clear error. |
| S4.5 missing-usage falls back to $0 | dispatcher::complete accepts `fallback_input_tokens` (the entry-cost ceiling). When upstream omits `usage`, that estimate is charged. AnthropicResp.usage made `Option<AnthropicUsage>` so a no-usage response parses cleanly. |
| S4.6 raw upstream errors leak to customer | `/router/complete` and `/billing/checkout` log raw errors at warn, return generic `upstream provider error; please retry` / `billing checkout failed; please retry`. |

### Stage 5 — 🟢 cleared in `eb92e1d`

| Codex blocker | Fix |
|---|---|
| S5.1 `ManagedPolicy::local_only` routes local through cloud | Removed `local_only()` constructor and `force_local` field. ManagedPolicy never produces `bluey-managed-local`. Test `managed_never_emits_managed_local_provider` scans every TaskType×LatencyLane and asserts no path produces local. Callers needing local-only must construct `LocalFallbackPolicy` directly. |
| S5.2 managed billing errors silently fail-over to direct | New `LlmError::Billing(String)` variant. `should_failover()` and `is_retryable()` both return false for Billing → LlmRouter returns terminal. BlueyManagedProvider maps `Unauthorized` / `TrialEnded` / `InsufficientBalance` → Billing. Test `billing_error_does_not_failover` proves a managed-then-direct chain returns Billing without reaching the second provider. |
| S5.4 (nit) hard-coded reload URL | BlueyManagedProvider now uses the server-provided `reload_url` from `InsufficientBalance { reload_url, .. }` so staging / custom-domains work. |

### Stage 6 — 🟢 cleared in `7a2a356`

| Codex blocker | Fix |
|---|---|
| S6.1 webhook idempotency not atomic with crediting | Two-layer guard: (1) `stripe_webhook_events` row dedupes the event; (2) `balance::credit` dedupes by `stripe_charge_id` INSIDE its transaction. Crash anywhere → next delivery re-runs everything safely (all steps idempotent). New unique partial index on `credit_batches.stripe_charge_id` (migration 0008). |
| S6.2 `balance::credit` not atomic | INSERT credit_batches + UPDATE accounts.balance_cents now in one transaction. Return type changed to `Result<bool>`: true=credited, false=duplicate-no-op. Two new tests: idempotency on same charge_id and atomic happy-path balance + batch count check. |
| S6.3 stripe_payment_method_id never persisted | `handle_checkout_completed` reads `session.payment_intent.payment_method` (when expanded) and persists it via `UPDATE accounts SET stripe_payment_method_id = COALESCE(...)`. Stage 7 auto-top-up will charge against this saved method. |
| S6.4 admin gating | Same as Stage 2 S2.2; cleared in `5743465`. |
| S6.5 (nit) signature timing attack + replay | Constant-time compare via `subtle::ConstantTimeEq`. Timestamp tolerance: events outside ±5 min rejected with explicit `tolerance` error. New test `signature_rejects_stale_timestamp` proves a 2023 epoch is rejected. |
| S6.6 (nit) raw stripe response leaks to customer | Stripe HTTP errors logged at warn with status+body; customer sees `billing provider unavailable; please retry`. |

### Stage 7 — 🟢 cleared in `d9253e1`

| Codex blocker | Fix |
|---|---|
| S7.1 `/usage/event` not idempotent | Migration 0009: `UNIQUE (account_id, request_id, kind)` on `usage_events`. `usage::record` uses INSERT OR IGNORE, returns `Result<bool>`. `/usage/event` returns 202 on first ingest, 200 on dedupe replay, 500 on DB error. Two new tests: same-id+same-kind dedupes, same-id+different-kind allowed. |
| S7.2 timestamp text-compare boundary bug | Both `/account/usage` queries now use `WHERE ts >= datetime('now', '-7 days')` matching the schema's `datetime('now')` default. No more mixed-format text comparison. |

### Stage 8 — 🟢 cleared in `0a3829e`

| Codex blocker | Fix |
|---|---|
| S8.1 hidden legacy login writes AccountConfig but usage/credits read keyring | After `save_account` succeeds, `cue_login` also calls `CloudClient::with_default_keyring().save_tokens(Tokens { access, refresh, email: user_id })`. Best-effort: keyring failure prints a clear warning but does not fail login. The hidden support path now feeds both stores. |
| S8.2 `bluey credits` over-promises | CLI help softened from "Show credit-batch expiration info" to "Show your current Bluey balance and 1-year credit-validity reminder. (Per-batch expiration listing is not yet available; coming in a future release.)" |

## Codex re-verification commands

```bash
# Stage 2 admin gating
cargo test -p bluey-server admin -- --nocapture
# → 3 tests pass: rejects_unauthenticated, rejects_non_admin, allows_admin

# Stage 4 idempotency
cargo test -p bluey-server idempotency
# → 5 tests pass: fresh_reservation_then_in_progress_on_replay,
#                 cached_complete_returns_payload_on_replay,
#                 release_allows_retry, failed_is_terminal,
#                 different_accounts_share_request_id_namespace_safely

# Stage 4 trial floor
cargo test -p bluey-server trial_seconds_min_1s
# → 1 test passes; sub-second request consumes 1s of budget

# Stage 5 Billing terminal
cargo test -p cue-llm billing_error
# → 2 tests pass: does_not_failover, terminal_helpers

# Stage 5 ManagedPolicy never-local
cargo test -p cue-router managed_never_emits_managed_local_provider
# → 1 test passes; scans 6×3 task/lane combos

# Stage 6 atomic credit + idempotent webhook
cargo test -p bluey-server credit_idempotent_on_same_stripe_charge_id
cargo test -p bluey-server signature_rejects_stale_timestamp
# → both pass

# Stage 7 usage idempotency
cargo test -p bluey-server record_idempotent_on_same_request_id
cargo test -p bluey-server different_kinds_per_request_id_allowed
# → both pass

# Full pipeline
cd /Users/uno/Downloads/cue && cargo test --workspace --all-targets
# → 402 passed
cd server && cargo test
# → 44 passed
```

## Outstanding nits NOT addressed

These are flagged in per-stage reviews as 🟡 nits, not blockers; deferred:

- **S5.3** `estimated_input_tokens: None` always in BlueyManagedProvider.
  The router/session-level token estimate isn't threaded down to the
  LlmProvider trait yet. Will land alongside the daemon
  ProviderRegistry rewire.
- **S7.3** `/usage/event` trusts client-supplied cost fields. Server-
  recorded router events are the authoritative source for spend
  projections; the client-event data path should be marked as
  untrusted analytics in v0.2.x.
- **S7.4** tier classification originally tied to the fractional-cent
  doc; reconciled by S4.3 to use whole cents.
- **S8.3** tier projections hard-coded in CLI. A `/pricing/tiers`
  endpoint would let the server own the tier numbers; v0.2.x.
- **S8.4** `cue-cloud-client::Error::Server` body surfaced verbatim
  to CLI callers. Will sanitize alongside the ProviderRegistry rewire.

## Next stage gate

Codex green-light on all stages enables:

1. **ProviderRegistry rewire** — daemon swaps to BlueyManagedProvider
   when a token is present in keyring; `BLUEY_DEV_BYOK=1` env var
   gates legacy direct providers for dev. Closes S5.3 and S8.4.
2. **Stripe auto top-up** in `/router/complete` post-deduction. Uses
   the now-persisted `stripe_payment_method_id` from S6.3. Charges
   off-session via `payment_intents.create` against the saved PM.
3. **Streaming proxy** through bluey-server for the deep lane.

These three are the remaining pieces between current state and a
v0.2.0 alpha release candidate.
