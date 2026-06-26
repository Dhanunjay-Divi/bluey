# Round 198 - Web Search Paid Quota Policy

## Trigger

Owner questioned the proposed paid web-search cap:

`Paid account: 50 searches/day why for paid cap because theyre paying right?`

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 15:34 EDT

## Finding

- `50 searches/day` is not currently enforced in product code.
- Current server web search is still an opt-in managed lane guarded by env configuration.
- Existing docs only say account/day search quotas should be added before broad production use.
- A low hard daily cap for paid users would feel wrong because paid users are already paying for usage.

## Policy Decision

Paid web search should be credit-metered and abuse-guarded, not blocked by a small fixed daily count.

Recommended model:

- Free/trial:
  - small hard daily search cap
  - conservative query and fetch limits
  - no background/bulk search
- Paid credit accounts:
  - charge/reserve credits for search provider cost, fetched-page processing, and answer tokens
  - no ordinary `50/day` product cap
  - keep short-window rate limits to stop runaway loops
  - keep a high daily circuit breaker for fraud/automation, support-overridable
  - allow user/workspace spend controls such as daily search spend limit or low-balance stop
- Admin/ops:
  - log search attempts, source fetch count, cost, cache hits, and rejection reason
  - flag suspicious account/device/IP/domain velocity
  - block scraping-style use even when credits exist

## Why Any Paid Guard Still Exists

Paid does not mean unbounded provider risk.

Bluey still needs guards because web search can:

- burn upstream search/fetch/model cost very quickly
- be automated for scraping
- hit provider rate limits
- create privacy risk if sensitive queries are sent out
- generate refund/dispute risk if a bug loops searches

Those controls should feel like safety rails, not like a low usage cap.

## Product Copy Direction

Avoid:

- `Paid: 50 searches/day`

Use:

- `Paid: web search uses credits, with safety limits to prevent runaway or abusive usage.`
- `You can set a daily search spend limit.`
- `Bluey shows when it searches and cites sources.`

## Implementation Notes

Before broad production web-search enablement:

- Add durable account/day usage accounting for search attempts and successful provider calls.
- Add idempotency so answer retries do not double-charge or double-count searches.
- Add per-account and per-device short-window rate limits.
- Add a paid-account daily spend guard, not just a count guard.
- Add admin override or higher tier limits for legitimate heavy use.
- Add source/result caching for repeated queries to reduce cost and duplicate provider calls.

## Current State

- No code changed in this round.
- The active code still has managed web search disabled unless the server is configured with search env vars.
- The next implementation round should convert this policy into server-side accounting, credit reservation, and UI copy.
