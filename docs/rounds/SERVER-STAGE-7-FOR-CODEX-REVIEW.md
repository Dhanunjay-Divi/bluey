# Server Stage 7 — Codex Review Asks

> **Commit:** `ed5d711 feat(server): real /account/usage aggregation + /usage/event ingestion (Stage 7)`

Customer-facing usage breakdown for the dashboard. Rolling 7-day
aggregation, tier classification, projected duration. `/usage/event`
ingestion is wired so daemon-emitted events actually get stored.

## What I verified locally

- `cargo build` clean.
- `cargo test` server 30 unchanged.

## What I want you to review

### 1. /account/usage aggregation

- Window: rolling 7 days from now.
- Returns:
  - `total_cues`, `total_cents_spent`
  - `mix`: per-bucket entries (task_type, count, cost_cents, percent)
    sorted by cost desc.
  - `tier_label`: Light / Typical tech / Heavy based on
    `cues_per_30 = 3000 / avg_cost_per_cue`.
  - `projected_days_remaining`: balance / daily-burn rate, capped at
    365 when burn is zero.

**Ask:**
- Tier thresholds: Light ≥ 2200 cues/$30, Typical 1100-2200, Heavy <
  1100. These come from the PRICING-MODEL.md three-tier worked
  example. Comfortable, or do you want adaptive thresholds based on
  observed user distribution?
- Bucketing: groups by `COALESCE(task_type, lane, 'general')`. v0.1
  daemon emits `lane` reliably but not always `task_type` (the
  classifier metadata isn't currently forwarded to usage_events from
  request_cue). Is the COALESCE the right fallback, or should we
  filter out missing-task_type rows?
- Projection: if balance_cents > 0 but no usage in window, returns 365
  days. That's a sensible "credits last forever at this rate" upper
  bound. Confirm.

### 2. /usage/event ingestion

- Auth-bound (lives behind the protected sub-router).
- Inserts into `usage_events` table.
- 202 Accepted on success, 500 on DB error.

**Ask:**
- The daemon emits events via fire-and-forget; if the server is down
  the daemon currently doesn't queue them. Add a small SQLite-backed
  spool on the daemon side? My read: defer to v0.2.x — packet loss
  here means metering inaccuracy, not user-visible breakage.
- Server-side dedup: `request_id` is in the event body but we don't
  enforce uniqueness. A daemon retrying after a flaky network would
  double-count. Worth adding `UNIQUE (account_id, request_id)` index
  + INSERT OR IGNORE? Probably yes for v0.2.

### 3. Tier classification accuracy

Tier comes from "your last 7 days" not "your forever average". A new
user with one expensive Hard query in their first hour gets labeled
"Heavy" until they do enough other things. Acceptable v0.2 behaviour,
or window the tier label more aggressively (e.g. require ≥ 20 cues
before classifying)?

## What's NOT in Stage 7

- Auto-top-up firing inside `/router/complete` (depends on Stripe;
  next stage).
- Per-tenant rate limiting / budget caps.
- Spool / retry logic on daemon side.
- Server-side dedup on request_id.
