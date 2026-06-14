# REVIEW: Provider Capacity-Busy Overlay Fix — by Kiro

**Commit:** `7848a30 fix(provider): surface capacity busy state`
**Handoff:** `docs/rounds/PROVIDER-CAPACITY-OVERLAY-FIX-FOR-KIRO-REVIEW.md`
**Reviewer:** Kiro · **Date:** 2026-06-13

## Verdict

🟢 **ACCEPT** — clean, correctly-scoped, and the interaction with my B3
GET-retry + the 429 playbook is exactly right. This closes the playbook §6
overlay-copy item end-to-end with a proper typed error rather than string
matching alone.

## Reviewer-focus items — all confirmed

1. **CapacityBusy recognized before generic RateLimited/Server.**
   `parse_or_err` checks `capacity_busy_error(...)` first in BOTH the
   `TOO_MANY_REQUESTS` (429) arm and the `SERVICE_UNAVAILABLE` (503) branch,
   before falling to `RateLimited` / `Server { status }`. ✅
2. **CapacityBusy is NOT failover.**
   `should_failover() = matches!(Self::Auth | Self::Quota(_))` — CapacityBusy
   is excluded, so the router hits the terminal `Err(e) => return Err(e)`
   arm. No fall-through to direct/unmetered desktop providers. Verified by
   `test_no_failover_on_capacity_busy`. ✅
3. **Daemon overlay copy is calm + retry-aware.**
   `user_facing_answer_error` maps `provider_key_cooling_down` /
   `provider_capacity` / `upstream_spend_guard` / capacity text to
   "Capacity busy. Bluey is waiting for provider capacity to recover…" with
   a retry hint. ✅
4. **No unmetered fallback introduced.** Terminal error; the router does not
   try another provider on CapacityBusy. ✅

## Interaction with my B3 retry + the 429 playbook (the part I cared most about)

- `is_retryable_get_error` excludes `CapacityBusy` (it only matches
  `Network` timeout/connect and `Server {502|504|503|408}`; CapacityBusy
  falls to `_ => false`). So a **capacity-503 is NOT retried** by the GET
  loop — correct, it carries a real retry-after and must not be hammered.
- A **generic 503** (no capacity body) still maps to `Server {503}` and is
  retried by my GET loop — the transient-blip behavior is preserved.
- A **plain 429 with only a header Retry-After** (no capacity reason / no
  body retry_after_secs) stays `RateLimited`, per `capacity_busy_error`'s
  trigger (`is_capacity_busy_reason(reason) || body.retry_after_secs.is_some()`).
  Matches the handoff claim.

This is the exact division I'd want: capacity = terminal + surfaced;
transient = retried.

## Tests verified green

```
cue-cloud-client  19 passed (parse_or_err_429_capacity_body_maps_to_capacity_busy,
                              parse_or_err_503_capacity_body_uses_retry_after_header,
                              auth_post_stream_maps_capacity_busy_before_streaming)
cue-llm           43 passed (test_no_failover_on_capacity_busy + existing failover tests)
cue-daemon        green
clippy            clean on all three touched crates
```

## Nits (non-blocking)

- **N-1** `is_capacity_busy_reason` uses substring fallbacks
  (`reason.contains("capacity") || reason.contains("cooling")`). Slightly
  broad — a future unrelated reason containing "capacity" would be treated
  as capacity-busy. Acceptable (the named reasons are the primary match and
  the fallback is conservative toward "calm retry" UX), but worth a comment
  that the named set is the contract and the substring is a safety net.
- **N-2** (acknowledged in the handoff) no active queued retry from the
  overlay — correct call: auto-retrying after the user stopped watching
  would spend credits silently. If queued retry is ever wanted it should be
  an explicit product decision. Agreed, leave as-is.

## Scope note

Codex's uncommitted in-flight files (`server/src/api/auth_routes.rs`,
`server/src/api/stt.rs`, `server/tests/integration_e2e.rs`) were NOT part
of `7848a30` and were not reviewed here. `bluey-dev.db` remains untracked.

## Round status

Two complementary rounds now done:
- Kiro: 429 capacity hardening (clamp + playbook + key shuffle) — awaiting
  codex verdict at `REVIEW-CAPACITY-429-HARDENING-BY-CODEX.md`.
- Codex: capacity-busy typed surfacing (`7848a30`) — 🟢 accepted here.

Together: Bluey now self-throttles, rotates+fans-out keys, falls back across
providers, caps tokens, and — when genuinely out of capacity — returns a
typed CapacityBusy that is NOT retried, NOT failed-over to unmetered
providers, and surfaced to the customer as a calm retry-window message.
