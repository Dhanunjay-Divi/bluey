# FIX-711: Many-Matches Preview Broke Application References

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the local Jobs portal preview fixture and
> synthetic `many-matches` scenario. This is public preview data only; no authenticated workspace,
> customer application, provider, deployment, or production flag was used.

## Issue

The synthetic `many-matches` scenario replaced the fixture's original jobs with volume rows while
applications and related evidence still referenced the original job IDs. One base outcome event
also named an application whose job did not match the event's job.

## Root Cause

The scenario generated all 125 rows from scratch instead of retaining the four parent jobs already
referenced by applications. The fixture had no referential-integrity regression spanning matches,
applications, evidence, browser sessions, interventions, and candidate events.

## Fix Summary

- Preserve the four original matches and append only enough deterministic synthetic rows to reach
  exactly 125 unique matches.
- Keep stable synthetic IDs, canonical keys, external IDs, and Track bindings without mutating the
  source fixture.
- Bind the outcome event to the application that actually references its job.
- Add regressions covering every application-related parent reference and deterministic repeated
  scenario generation.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/data/preview.ts` | Preserve parent jobs, correct the outcome application, and append deterministic volume rows |
| `jobs/portal/src/data/preview.test.ts` | Prove job/application/evidence/session/intervention/event referential integrity |

## Edge Cases Handled

- all four existing application job IDs remain present after expansion;
- application evidence always references an existing application;
- optional browser-session and intervention application IDs remain valid;
- candidate event job and application IDs resolve to the same parent relationship;
- exactly 125 unique match IDs are returned; and
- repeated generation is deterministic and leaves the base fixture unchanged.

## How to Test

```bash
(cd jobs && npm run test --workspace @bluey/jobs-portal -- src/data/preview.test.ts)
# Observed locally: 3 / 3

(cd jobs && npm run test --workspace @bluey/jobs-portal)
# Observed locally: 349 / 349 across 28 files
```

## Known Limitations

- This fix applies only to public synthetic preview fixtures. It does not modify or validate
  authenticated customer workspace data, server persistence, or employer receipts.
- Local preview tests do not prove a deployed portal bundle, exact-tip CI, Docker/Linux,
  deployment, or production-flag state.
