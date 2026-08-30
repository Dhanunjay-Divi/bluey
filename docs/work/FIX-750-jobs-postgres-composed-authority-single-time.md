# FIX-750: Jobs PostgreSQL Composed Authority Single-Time Evaluation

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Severity:** P1 freshness-race blocker

**Status:** Source fix implemented; live contention and aggregate evidence pending

## Issue

PostgreSQL effect paths could resolve source and ATS authority at one database timestamp, wait for
the job-integrity publication fence, then resolve signed integrity at a later timestamp. An ATS
binding that expired during that wait could remain represented as active when the effect decision
was made.

## Root Cause

The non-`_at_ms` composed resolver sampled time while resolving Phase 614 source authority before
it acquired the integrity publication fence. Its signed-integrity resolver later acquired that
fence and sampled a second time. Eligibility trusted the already-materialized ATS status, so the
three authorities did not share one post-lock freshness boundary.

## Fix Summary

- After the caller-owned `H -> M -> ATS -> D` prelock, acquire the integrity publication fence.
- Sample one PostgreSQL `clock_timestamp()` only after every authority publication fence is held.
- Resolve source, ATS, and signed integrity through the existing `_at_ms`/no-relock paths using
  that exact timestamp.
- Remove the optional-time branch from composed PostgreSQL resolution so effect composition cannot
  silently return to split-time evaluation.
- Pin the fence, time sample, source, ATS, and integrity order in the static regression.

## Files Modified

| File                                                             | Change                                  |
| ---------------------------------------------------------------- | --------------------------------------- |
| `server/src/db/jobs/job_integrity_composition.rs`                 | One post-fence timestamp for composition |
| `docs/work/FIX-750-jobs-postgres-composed-authority-single-time.md` | Record defect and evidence ledger       |

## Edge Cases Handled

- Waiting behind integrity publication can no longer preserve an already expired ATS projection.
- Source and signed-integrity expiry are evaluated at the same timestamp as ATS expiry.
- The existing `H -> M -> ATS -> D -> integrity` lock order is preserved.
- Workspace representation callers that already own the integrity fence retain their explicit
  `_at_ms` no-relock path.

## How To Test

```text
postgres_composition_static_order_is_h_m_ats_d_then_source_ats_integrity    PENDING
Live PostgreSQL control-lock wait across ATS expiry with zero mutation       PENDING
Full composition, admission, workflow, Clippy, and release gates             PENDING
```

## Known Limitations

- The live race regression requires configured PostgreSQL and remains pending until the focused
  fixture can seed all signed source, ATS, and integrity authority without direct positive SQL.
