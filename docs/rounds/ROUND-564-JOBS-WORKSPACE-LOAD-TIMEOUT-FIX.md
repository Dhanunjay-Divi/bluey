# Round 564: Jobs Workspace Load Timeout Fix

Date: 2026-07-22

Status: implementation ready; deployment evidence to be appended after rollout

## Problem

A live Jobs visit produced a Cloudflare 524 page while opening the signed-in
Jobs portal. Public `/jobs/` and `/health` were fast, and unauthenticated
`/api/jobs/workspace` returned quickly with `401`. That points to the signed-in
workspace payload path rather than static hosting.

The expensive part of that path was account-level projection of shared global
discovery candidates. The workspace endpoint synchronously attempted to
materialize up to the shared candidate scan before returning the portal data.
When global candidate projection was slow, locked, or backed up, the first
signed-in render could wait long enough for Cloudflare to time out.

## Fix

`GET /api/jobs/workspace` and onboarding completion now schedule global
candidate materialization as best-effort background work instead of blocking the
request. The workspace payload returns the current account state immediately;
newly projected matches appear on the next refresh after the background pass
finishes.

The portal API client now uses bounded fetches:

- short timeouts for authenticated reads and token refresh;
- longer timeouts for writes/downloads;
- a clear retryable Jobs message instead of an indefinite blank loading screen.

This preserves the safety boundary: global feed entries remain discovery leads,
and employer-facing action still requires original-source revalidation and the
existing eligibility checks.

## Files

- `server/src/api/jobs.rs`
- `jobs/portal/src/api.ts`

## Verification

Local verification before deployment:

```text
cargo fmt --check
cargo check
cargo test jobs::
npm run typecheck --workspace @bluey/jobs-portal
npm run test --workspace @bluey/jobs-portal
npm run build --workspace @bluey/jobs-portal
```

Observed results:

- server `cargo check` passed;
- focused server Jobs tests passed: 115 passed;
- portal typecheck passed;
- portal tests passed: 10 files, 62 tests;
- portal production build passed.

The Vite build emitted only the existing large-chunk advisory and did not emit
source maps.

## Deployment Plan

Deploy the Jobs API/server code and the rebuilt Jobs portal as a paired release.
Before replacing production artifacts:

1. capture current service state and restart counters;
2. preserve the current Jobs API/server binary and Jobs portal directory;
3. confirm the three protected Jobs flags remain disabled:
   `BLUEY_JOBS_MODEL_GENERATION_ENABLED=0`,
   `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0`, and
   `BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0`.

After deployment verify:

- `GET /health` returns `200`;
- `GET /jobs/` returns `200` quickly;
- unsigned `GET /api/jobs/workspace` returns `401` quickly;
- public `/api/jobs/internal/discovery/lease` remains unavailable;
- missing Jobs source maps return `404`;
- production Jobs flags remain disabled;
- signed-in browser workspace renders without the minute-long wait.

## Rollback

For a portal-only regression, restore the saved `/var/www/bluey/jobs` directory.

For an API regression, restore the paired previous Jobs API/server binary and
repeat route, flag, health, and portal checks. Prefer fix-forward unless the
workspace route is unavailable or protected route behavior regresses.
