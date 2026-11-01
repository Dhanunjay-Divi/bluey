# FIX-835: Read PostgreSQL's managed-command marker as BOOLEAN

Preflight:bluey-ops; actual77890 SQLSTATE evidence, schema/source and private probe.

## Issue

Corrected-recovery run77890 passed all23 focused tests (13pure/5SQLite/3fixture/
2example), then real private-PG recovery returned500/18ms. Private diagnostic
reported42883/operator_type, no API panic and sampled clients idle/no blockers.

## Root Cause

PostgreSQL033 defines managed_cloud_authority_required BOOLEAN NOT NULL (line525).
New postgres_historical_session_command projected marker=1 copied from SQLite.
PostgreSQL has no boolean=integer operator. SQLite's separate INTEGER flag uses
that expression correctly. This is a typed SQL defect, not a session/canonical
authority mismatch and not a reason to relax recovery guards.

## Fix Summary

Change only the PostgreSQL projection to managed_cloud_authority_required IS TRUE.
Preserve SQLite=1, bool decoding, joins/filters, historical validation and lock order.
Add a source-linked Node regression binding the projection to the BOOLEAN migration
and preserving the distinct SQLite expression. Ordinary actual-PG recovery remains
the operational regression; parser proof alone does not qualify recovery.

## Files Modified

server/src/db/jobs/execution_session_authority.rs (one PG expression);
jobs/scripts/nonprod-runner-control-plane.test.mjs (typed-source parity regression).

## Edge Cases Handled

The Boolean projection returns true only for true; false/null return false. The
real migrated column is NOT NULL. No integer cast, schema rewrite or truthiness
coercion is introduced. Only SQL expression typing changes.

## How to Test and Observed Evidence

Source-derived private-PG17.10 prepare probe17989 TERMINAL0 reproduced original
42883 and prepared the identical corrected query successfully. It read the exact
historical PG SELECT literal and28-field WORKFLOW_COMMAND_SELECT from source,
using transaction-local temporary tables with BOOLEAN marker, TEXT identifiers
and BIGINT matching event/authority fences. This isolates grammar/operator typing,
not full migrated-schema/runtime authority. Separate true/false/null projection
cases returned true/false/false (3/3). No durable rows created; rollback/PG/root
cleanup passed,parentempty. Original SELECT SHAb602fcf318cd6adb22caa9f40631403f0b28ab7fa1a54d00277c9246e544be4c;
corrected SHA3646dc0565b3fbf7981784a9916167f4d5b7d89bd17e06cceb825fda0ce79e98.
Earlier probe13190 exited1 BEFORE SQL prepare because its identifier validator
omitted digits in source column names; corrected the diagnostic validator, cleaned
both roots and counted no original/corrected SQL result from that failed setup.

Node report/source tests187pass/0skip; scoped rustfmt/diff checks pass. Actual
corrected API build/recovery, independent review and full gates remain pending.
Use COMMAND-PHASE-638-CHECKPOINT-RECOVERY-GREEN.md's build/client preflight/API
segment on a fresh private cluster for each mode; do not claim a500 is a green red.

Actual77890 prior to correction: test compile2m53s, SQLite5 in21.39s, fixture3 in
0.00s, example build1.71s/2passes, API build2.51s. API SHA8a1c426d805bfe521991e0516a60a7305761e72e7e04067eccef5d14814b22d9;
fixture SHA63b0ee8575c770c6f136014fac2672f84055a4b313ea4f016fef6e69f08407b5.
774-source manifest0b03233dbbbecaa986fca2ee23877742a8a64e250ae3380a9aa6bb1477975ba7,
HEAD40949858/scopedstatus matched before/after; API/PG/root cleanup passed,
parentempty/38GiBfree. Privacy2928tracked/2652text passed before77890. The before
inspector and real claim/heartbeat were reached, but no recovery-success/replay
or final lifecycle assertions passed in that run.

## Known Limitations

23 focused passes remain their dated source scope. The one-expression correction
needs actual dual-mode PG recovery/replay and current-source preservation/full
qualification. No release, production/owner DB, browser or employer effect occurred.

Actual post-correction86077(review-first) and20772(auto) both TERMINAL0: real
prepared recovery and fresh exact recovery replay200, stablefrozen/state/ATS
authority, terminalfinishreplays and full6/7child/PG/root cleanup passed. Both use
774source d66a712f17e2cbced73fc1b2006836e771cc8abe7c5e5c2db2760861d932cb5d;
FIX832 records binary hashes/counts/remainingholds. This qualifies this Boolean
correction operationally, not the full source/production/release goal.
