# FIX-782: Jobs CI time budget

> **Codex preflight:** Loaded `$bluey-ops`, inspected the exact failing hosted run and current
> workflow, and did not use the SSD archive or touch production.

## Issue

The exact-PR-head Bluey Jobs CI run
[`33357647360`](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33357647360)
was cancelled by the combined Linux job's 45-minute timeout. Every completed gate was green, but
the timeout interrupted the 108-test server integration target after 89 tests had reported `ok`
and before either business-messaging simulator verifier step could run.

## Root Cause

The workflow retained a 45-minute job ceiling after its mandatory gate set grew. On the hosted
runner, the job needed 36 minutes and 43 seconds to reach the integration-test command. Its clean
integration target then compiled for 1 minute and 40 seconds and reported 89 passing tests with
zero failures before the job-level deadline terminated it. The preceding server Jobs unit step
alone reported 955 passing tests in 1,260.32 seconds, in addition to its compile time.

This was a deterministic CI capacity mismatch, not a failed product assertion or a runner-disk
failure. The log contains the GitHub cancellation marker immediately after the 89th passing
integration result, while the separate Darwin job completed successfully.

## Fix Summary

Set the combined Linux `jobs-ci` job timeout to exactly 90 minutes. This preserves a finite hard
stop while giving the current clean Rust compile/test path enough room to finish the remaining
integration and simulator checks. The existing pre-install CI guard now locates the exact
`jobs-ci` block and rejects a missing, duplicate, malformed, lowered, or otherwise changed timeout;
its negative self-test proves the prior 45-minute value is rejected.

No test command, test selection, feature, release authority, production flag, or runtime behavior
was removed or relaxed.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/jobs-ci.yml` | Raise the combined Linux job's closed timeout from 45 to 90 minutes. |
| `jobs/scripts/ci-guards-self-test.mjs` | Enforce the exact 90-minute job-scoped contract and prove the 45-minute regression is rejected. |
| `CHANGELOG.md` | Record the CI reliability correction without claiming a green rerun. |

## Edge Cases Handled

- The guard scopes the timeout to `jobs.jobs-ci`; the independent Darwin job keeps its existing
  20-minute bound.
- A missing or duplicate timeout fails closed.
- A non-integer, lower, or unreviewed higher timeout fails closed rather than silently changing the
  resource contract.
- The 90-minute ceiling remains bounded, so a real hang still terminates the hosted job.
- The cancelled run is not represented as passing; a fresh exact-tip hosted run remains required.

## How to Test

```bash
node jobs/scripts/ci-guards-self-test.mjs
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 \
  .github/workflows/jobs-ci.yml
git diff --check
```

After push, require a fresh exact-tip Jobs CI run to complete all 108 integration tests and both
business-messaging simulator verifier steps before changing the release verdict.

## Known Limitations

- The source guard proves the workflow budget contract, not hosted completion. PR #34 remains
  blocked until the fresh exact-tip Jobs CI run is green.
- Further mandatory gate growth may justify splitting the combined job in a separately reviewed
  phase. This fix does not hide such growth behind an unbounded timeout.
