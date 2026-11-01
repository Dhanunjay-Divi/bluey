# FIX-823: Session regression fixtures use undersized run IDs

## Issue

Focused run23220 compiled, passed eight pure tests, and failed all three new
SQLite cases before command admission. The production canonical START builder
returned invalid workflow command request; managed fixtures and actual client
lifecycle were not reached because the shell gates stopped at the failure.

## Root Cause

The reused legacy execution fixture derives cloud-run-SUFFIX. The short distinct,
scope and unstarted suffixes produced IDs shorter than the canonical workflow
protocol's20-byte minimum. The direct legacy fixture never stages a workflow;
this was a new test composition error, not a production resolver failure.

## Fix Summary

Prefix only the new SQLite fixture suffix with session-authority- and assert the
derived run ID meets the existing production opaque-ID validator. Keep that
validator, real session/run distinction and every production authority unchanged.

## Files Modified

server/src/db/jobs/execution_session_authority_tests.rs only.

## Edge Cases Handled

Each distinct/pre-start/scope case now uses a canonical-length synthetic run ID.
The canonical START builder still computes the request/result IDs and rejects
invalid protocol inputs; no direct trusted command/session rows are fabricated.

## How to Test

Repeat COMMAND-PHASE-638-SESSION-BINDING-REGRESSIONS.md only after reviewed
prospective privacy/whitespace checks. Run23220 exit101 is retained as failure:
2m33s test compilation,8 passed/3 failed, no skips,9.91s test time. Private PG
exited, exact root removed and parent empty;38GiB free.774-source manifest
4520f7d4473dc933dc9017b27fe8fcae0502f1c68a87b00ef53ed63479037347
and HEAD/scoped status matched before/after. The correction has not yet run.

Rerun1656 passed canonical START construction in all cases and the independent
unstarted rejection. It then exposed the separate unmanaged-dispatch test setup
defect FIX824. The ID-length prerequisite is locally resolved; this does not
qualify positive resolver or actual client behavior.

## Known Limitations

No actual prepared-client claim/heartbeat/release evidence or full source gate
is provided by the failed run. Production and all external accounts remain untouched.
