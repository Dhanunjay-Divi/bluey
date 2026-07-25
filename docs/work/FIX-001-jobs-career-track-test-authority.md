# FIX-001: Jobs Discovery Fixtures Bypassed Career Track Authority

## Issue

Six discovery and workspace tests created Career Tracks without the verified
application identity now required by the production path.

## Root Cause

Legacy tests called `jobs::upsert_track` with a minimal `test_track`. The new
Career Track invariant correctly rejected that fixture because it did not
represent a usable application identity or current source resume.

## Fix Summary

Added one test-only API helper that provisions the account-scoped verified
identity and current resume authority before storing a Career Track. Replaced
the six direct legacy fixture writes with that helper.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs.rs` | Added and used `store_discovery_test_track` |

## Edge Cases Handled

- Each test account receives its own verified identity.
- The track is bound to the account's current source resume.
- Production authority remains unchanged; only fixtures follow the real path.

## How to Test

```bash
cd server
cargo test --quiet
```

The full server suite passes, including 799 unit tests and 76 integration
tests.

## Known Limitations

- None.
