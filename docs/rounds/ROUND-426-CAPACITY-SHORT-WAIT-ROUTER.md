# ROUND-426 Capacity Short-Wait Router

Date: 2026-07-08

Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Problem

Bluey could show a visible `Capacity busy` answer for normal rapid follow-ups, even when the retry hint was only about one second. That looked broken in the overlay because the user had multiple model providers configured and expected Bluey to recover internally instead of asking them to retry.

## Root Cause

The managed router already tries candidate providers and keys in order, but two short windows could still leak to the UI:

- account-level LLM burst guard rejected before any provider dispatch
- every provider/key candidate in the selected lane was temporarily cooling, then the router returned immediately

The overlay then rendered a generic provider-capacity message, which made a one-second pacing event look like a hard provider outage.

## Changes

- Added `BLUEY_CAPACITY_SHORT_WAIT_MAX_SECS`, default `2`, capped at `10`.
- Account LLM burst checks now wait once for short cooldowns, then retry before returning a capacity response.
- Streaming and non-streaming provider route scans now wait once for short all-candidate capacity windows, then rescan the route list.
- Final capacity failures now record an `answer_capacity_busy` ops event with:
  - lane and effective lane
  - streaming mode
  - retry-after seconds
  - route candidate count
  - capacity retry count
  - request/session refs

## User Impact

Rapid follow-up asks should no longer show a visible `Capacity busy` card for one-second cooldowns. Bluey will absorb that small wait server-side and keep trying the healthy provider/key path.

If capacity is genuinely exhausted after the short wait, support logs now make the cause traceable from the request/session refs instead of guessing whether the block was account pacing, key cooldown, or provider capacity.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml short_capacity_wait_default_and_override --quiet`
- `cargo test --manifest-path server/Cargo.toml router_complete_short_waits_account_llm_burst_guard --test integration_e2e --quiet`
- `cargo test --manifest-path server/Cargo.toml router_complete_falls_back_when_preferred_provider_429s --test integration_e2e --quiet`
- `cargo test --manifest-path server/Cargo.toml router_complete_reports_upstream_error_after_capacity_skip --test integration_e2e --quiet`

No signed deploy or release was performed in this round.
