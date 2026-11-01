# FIX-824: Session regression command is not eligible for managed dispatch

## Issue

Focused run1656 compiled and passed9 tests but failed two started SQLite cases:
claim_jobs_workflow_command returned None. No resolver-positive, managed fixture
or actual prepared-client journey was reached by that stopped recipe.

## Root Cause

The new DB-test helper admitted its canonical START with managed-required=false.
Production dispatch intentionally selects only managed-required=true commands
before request-start. The test combined a legacy command with the real managed
dispatcher; the latter correctly refused it. This is test composition, not a
production dispatch defect, and its fail-closed condition must not be relaxed.

## Fix Summary

Use the existing synthetic managed-runtime lifecycle fixture, exact managed
workflow binding and managed request-start function in the new test helper.
Expose only the existing test-only scoped environment guard and fleet-readiness
lifecycle helper to the sibling test module; reset the environment on drop and
serialize these environment-sensitive tests. Reuse canonical signed runtime and
inventory/cutover APIs, including readiness before binding. Recover the fixture's
original authority clock by a read-only exact head/manifest lookup and the existing
helper's documented5000ms issued-at offset, preserving shared policy lifetimes.
No trust-row SQL, production behavior, schema, flag default or authority changes.

## Files Modified

server/src/db/jobs/execution_session_authority_tests.rs; test-only visibility of
tests.rs ManagedCloudAdmissionEnvironment/staging and runner_volume_purge_tests
mark_current_fleet_ready. Separately, the Node client's injected fetch now records
closed method/status/elapsed-time metadata for actual worker HTTP rather than
leaving the last ordinary queue observation in failure diagnostics; no raw body.

## Edge Cases Handled

Canonical-length run IDs remain pinned by FIX823. Pre-start rejection stays
separate from started exact-scope positives/negatives. Admission and real
request-start must succeed before claiming any session-resolver evidence.

## How to Test

Repeat the reviewed COMMAND-PHASE-638-SESSION-BINDING-REGRESSIONS.md after full
prospective privacy/whitespace. Prior1656 exit101:2m32s compilation,9 passed/
2 failed/zero skips,10.01s test time; actual client gates were not reached.
Private PG exited, exact root removed/parent empty,38GiB free.774-source manifest
21265467cef51431249083d335fd456e00b17c703422ca23a71f062cb6516b35
and HEAD/scoped status matched before/after. Correction runtime remains pending.

Correction qualified in84098: all3 SQLite cases,8 pure session tests and2 existing
managed authority cases passed (13 total, zero skipped). This closes the fixture
composition defect only. The subsequent actual ordinary PG claim returned500,
so integrated PG resolution/client lifecycle remains unqualified under FIX825.

## Known Limitations

No prepared-client or full source/release acceptance. Production untouched.
