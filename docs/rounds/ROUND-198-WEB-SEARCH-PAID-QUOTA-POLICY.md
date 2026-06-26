# Round 198 - Web Search Paid Quota Policy

## Trigger

Owner asked to remove low fixed paid search-cap framing and make paid search
credit-based.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 15:34 EDT

## Finding

- A low fixed daily paid-search cap is not currently enforced in product code.
- Current server web search is still an opt-in managed lane guarded by env configuration.
- Existing docs only say account/day search quotas should be added before broad production use.
- A low hard daily cap for paid users would feel wrong because paid users are already paying for usage.

## Policy Decision

Paid web search should be credit-metered with clear spend controls, not blocked
by a small fixed daily count.

Recommended model:

- Free/trial:
  - small hard daily search cap
  - conservative query and fetch limits
  - no background/bulk search
- Paid credit accounts:
  - charge/reserve credits for search provider cost, fetched-page processing, and answer tokens
  - no ordinary small fixed daily search-count cap
  - keep short-window controls to stop accidental repeated searches
  - allow user/workspace spend controls such as daily search spend limit or low-balance stop
  - keep support-overridable service-protection pauses for unusual account activity
- Admin/ops:
  - log search attempts, source fetch count, cost, cache hits, and rejection reason
  - monitor unusual account/device/IP/domain velocity
  - keep enforcement wording internal, not in customer-facing copy

## Why Any Paid Control Still Exists

Paid search still needs account-friendly controls.

Bluey should keep controls because web search can:

- spend credits quickly if a client repeats the same request
- hit provider capacity or reliability limits
- send sensitive-looking text to an external search provider if not sanitized
- surprise customers if there is no daily spend control

Those controls should feel like account protection, not like a low usage cap.

## Product Copy Direction

Avoid:

- `Paid: fixed daily search count`
- internal enforcement language in customer-facing copy

Use:

- `Paid: web search uses credits.`
- `You can set a daily search spend limit.`
- `Bluey shows when it searches and cites sources.`
- `If a request repeats too quickly, Bluey may briefly pause web search and use available context.`

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
