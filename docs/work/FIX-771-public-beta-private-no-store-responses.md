# FIX-771: Volatile public-beta access responses were cacheable

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

An admitted, denied, suspended, unavailable, or master-off response could be
reused after an account or cohort transition, leaving the portal with stale
access state.

## Root Cause

The beta status and administrative response helpers did not set explicit cache
headers, and the portal used the browser's default fetch cache behavior.

## Fix Summary

Every beta access, master-off, administration success, and administration error
projection now carries `Cache-Control: private, no-store, max-age=0`,
`Pragma: no-cache`, and `Vary: Authorization`. An outer response middleware
applies the same headers to the exact beta-status path even when authentication
rejects the request before its handler runs. The portal requests beta status
with `cache: "no-store"`. Closed response-schema validation remains unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs_beta_access.rs` | Apply and test private non-storable response headers |
| `server/src/api/jobs_operations.rs` | Cover pre-handler beta-status and administrator responses in outer middleware |
| `server/tests/integration_e2e.rs` | Prove unauthenticated 401 headers in shared and standalone routers |
| `jobs/portal/src/api.ts` | Disable browser caching for beta status |
| `jobs/portal/src/public-beta-api.test.ts` | Assert the fetch cache contract |
| `docs/work/FIX-771-public-beta-private-no-store-responses.md` | Record diagnosis and verification |

## Edge Cases Handled

- The unauthenticated 401, master-off 404, and database-unavailable 503 are non-storable.
- Account-specific administrator projections vary on authorization.
- Client parsing still rejects extra fields and inconsistent state/reason pairs.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-portal -- --run \
  src/public-beta-api.test.ts src/components/PublicBetaGate.test.tsx
npm run typecheck --workspace @bluey/jobs-portal
```

Observed locally on 2026-08-30: 15 focused portal tests passed and the portal
TypeScript check passed.

## Known Limitations

- Deployed CDN/proxy header read-back remains part of the dark-deploy smoke.
