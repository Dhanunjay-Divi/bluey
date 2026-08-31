# FIX-775: Public-beta metrics accepted impossible live enrollment counts

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The metrics renderer validated the durable hard cap and cumulative assigned
count, but it could emit a live enrollment count greater than the cumulative
number of slots ever assigned.

## Root Cause

`append_jobs_public_beta_metrics` required live, public, and administrator
counts to be non-negative and internally additive. It did not enforce the
durable cohort invariant `live_enrollments <= assigned_count`.

## Fix Summary

The renderer now accepts the live enrollment count only in the closed interval
from zero through the cumulative assigned count. A focused negative fixture
proves an otherwise internally consistent `live=2, assigned=1` snapshot fails
closed instead of publishing misleading operational evidence.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/metrics.rs` | Enforce the durable live-versus-assigned invariant and test the impossible state |
| `docs/work/FIX-775-public-beta-metric-count-invariant.md` | Record diagnosis and verification |

## Edge Cases Handled

- Negative live counts remain rejected.
- Deleted accounts may make live enrollment lower than assigned slots; that
  valid cumulative-retention state remains supported.
- Public plus administrator enrollment counts must still equal the live count.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  api::metrics::tests::invalid_or_private_metric_dimensions_fail_closed
```

## Known Limitations

- This validates the exported snapshot. Database read-back and alert delivery
  remain separate hosted release evidence.
