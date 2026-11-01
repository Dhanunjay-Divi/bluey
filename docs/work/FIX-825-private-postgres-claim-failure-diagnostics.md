# FIX-825: Private claim500 lacks a retained database error category

## Issue

Run84098 compiled and passed all13 focused Rust tests, then the real ordinary
API/client claim returned500 in1283ms. The existing API panic fingerprint was
empty (no panic observed);18 sampled PG connections were idle with no blockers.
The underlying storage error is not yet established. Do not infer a SQL defect
or claim resolver/lifecycle acceptance from this result.

## Root Cause

Production internal() logs typed failures through tracing to stdout, whereas
the previous diagnostic fingerprint covers only panic stderr. The owned cluster
already records PostgreSQL errors privately, but no closed category was captured
before canonical cleanup removed the root. Diagnostic coverage is incomplete.

## Fix Summary

On failure only, read the exact physically guarded private cluster log using the
existing no-follow/owner/mode/size/identity-bound file reader after API shutdown.
Emit only detected, explicitly logged SQLSTATE when present, a fixed category
and a bounded log hash. Never emit native error text, SQL, names, values or
connection/candidate content. Missing/unreadable evidence staysnull, not success.
No API response, production logging, SQL or authority behavior changes.

A suggested lock-before-LIMIT syntax cause was explicitly ruled out by actual
private PG17.10 probe86871: both clause orders passed2/2 on a temporary table,
rolled back; PG/root cleanup complete. Do not change that valid SQL based on
speculation. The actual500 remains unknown until new diagnostic evidence.

## Files Modified

jobs/scripts/nonprod-runner-control-plane.mjs and its report/diagnostic tests.

## Edge Cases Handled

Ambiguous/missing columns, missing relations, syntax/parameter/operator/input/
constraint/deadlock categories; absent or unclassified native errors; explicit
versus absent SQLSTATE. Three pure tests verify no private text disclosure.

## How to Test

57 Node tests pass, zero skipped. Actual PG diagnostic capture remains pending.
Run84098 exit1 is retained as failure despite13 Rust passes (11 new in13.35s,
2 existing managed cases in5.29s/5.99s). Rust test build2m43s; API/fixture2.60s.
API SHA5fbf6c87e3d4b50bd3f0db980484e245d15a5257f72bad11fc797ba37871f23e;
fixture SHA9e3cbe7273cf7ae525f15f8eba503ca454bfa396d9e0842e548d51244a25caa5.
Private API cleanuptrue/PGexited/rootgone/parentempty,38GiBfree.774-source manifest
3e1d860dd74378035b7e7e8926220c6e9b19d460cb198a3d31e0c5c6323b765e and
HEAD/scoped status matched before/after. Prepared lifecycle remains unproven.

Actual diagnostic82341 observed the private PG error with detectedtrue,
SQLSTATEnull/categoryother, tail SHAa7a7be7fadf4c5729b1ef32f59b231deb4a77db6c18abaf2df72cf8167cac26c.
Claim500/1439ms was reproduced; API/PG/root cleanup passed. Source774 manifest
cb7cb3753e0e5e9e845ebad628403d6d8e34b04d2a778dde3dc74d41c4e2eeb3 and
HEAD/scoped status matched. Capture is proven, but error classification remains
insufficient for a root-cause fix; obtain explicit safe SQLSTATE/error-leaf metadata
inside the physically verified private cluster next. No raw log disclosure.

Follow-up instrumentation: only after exact data-directory/host/port/database/user
identity passes, set log_error_verbosity=verbose on the disposable bluey_jobs_test
role and verify it through a new connection before API startup. The entire cluster
is launcher-owned and removed; no owner/database/production setting changes.
This supplies explicitly logged SQLSTATE to the existing closed parser instead
of trying to infer it from generic text. Runtime qualification remains pending.

Actual verbose diagnostic45305 exited1 after a1m40s API/fixture build. The real
ordinary claim returned500 in1444ms. Private PG explicitly logged SQLSTATE42804
(datatype mismatch), categoryother; tail SHA
55327afa93ec2365718aabec9178b3a32baf0368d1d0d4c585fc9f4b2265e30c.
No API panic or sampled PG blockers were observed. API cleanuptrue and PG exited;
the exact source774 manifest34dee63f3ce92a48fba349ae1beaaadaf28aa276b0ad8fb63b044863d21117eb
and HEAD/scoped status matched before/after. API SHA
090eef95f23691374707139ff702ff2bce438be65f3f73e42e0a600235386fd2;
fixture SHAe119a8f696566fbf80ccbd67dbaa9d70a8779395838681b77aadae5b72bc0352.
SQLSTATE narrows the diagnosis, but does not identify the failing statement or
justify a source fix alone. Positive prepared lifecycle remains unproven.

## Known Limitations

This adds evidence collection, not a product fix for500. The remaining protocol
negative matrix, recovery seam and full source/release gates are still open.
