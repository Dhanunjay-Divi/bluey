# FIX-826: PostgreSQL runner finalizer inserts Boolean into SMALLINT

## Issue

The ordinary private API/real-client claim returned HTTP500 after the distinct
run/session correction. Verbose diagnostic45305 observed PostgreSQL42804. The
prepared lease lifecycle is not yet qualified.

## Root Cause

`finalize_runner_volume_lease_binding_postgres_tx` inserts `FALSE` into
`jobs_runner_account_subjects.legacy_unresolved`. Authoritative migration
`infra/postgres/server-runtime/024_jobs_runner_volume_purge.sql` defines this as
`SMALLINT` constrained to0/1, not PostgreSQL BOOLEAN. The existing PostgreSQL row
decoder usesi16, and the ordinary subject writer already converts bool toi16.
PostgreSQL rejects the statement during preparation even if ON CONFLICT would
otherwise leave an existing row unchanged. SQLite-only atomic binding tests did
not exercise this PostgreSQL-specific assignment. Earlier target404 also prevented
the ordinary path from reaching this finalizer.

## Fix Summary

Replace only this PostgreSQL literal with `0::SMALLINT`. Preserve schema, locks,
transaction boundary, predicates, conflict handling and Boolean meaning. No
current-authority or truthfulness guard is relaxed.

## Files Modified

- `server/src/db/jobs/runner_volume_purge.rs`: production literal and a pending
  actual-PostgreSQL finalizer regression.
- Phase638 implementation/handoff/review records and changelog.

## Edge Cases Handled

The correction is type-correct for both absent and already-existing subject rows.
Actual finalizer creation/replay and rollback qualification remain required; a
prepare-only pass is not sufficient proof of the complete claim.

## How to Test

Prepare-only causal probe31410 exited0 in a canonical launcher-owned, physically
verified loopback PostgreSQL17.10 cluster. It read the uniquely named finalizer's
exact INSERT and the unique subject-table DDL from migration024. Inside private
transactions it created only TEMP accounts/subject tables, preserving the original
subject columns, FK and checks. The original prepared INSERT failed with exact
42804, parsed from bounded private verbose stderr without printing native text.
A fresh connection with only FALSE replaced by0::SMALLINT prepared successfully,
deallocated and rolled back. No permanent row/account was created. PG exited0,
owned root removed/parent empty.774-source manifest
bfa6c5dbcd22a4f90470c0c00d3ec2806276dee30fcf061828f2398f32494153
and HEAD/scoped status matched before/after this probe, before the source fix.

Focused89311 subsequently exited0 with14/14 tests and no skips. The actual migrated
PG finalizer regression passed in4.34s: absent subject, mismatch rejection without
subject/residency/binding rows, typed zero, explicit transaction rollback, public
wrapper commit/decodedfalse, joined exact binding fields and unchanged replay.
The other13 focused tests also passed. Rust test build2m45s; each selected case had
a fresh private database, with no destructive authority reset. PG exited0, exact
root removed/parent empty,38GiB free.774-source manifest
0f33d8b9ea6a71d01613c969ed7d5d1fc961ef0da956f9231287872f1569bdf3
and HEAD/status matched. This qualifies the narrow finalizer regression, not the
ordinary managed HTTP lifecycle or full source.

Ordinary API61106 exited1 after1m23s build. Real managed-client claim returned400
in2ms, with no observed PG error, APIpanic or sampled blockers. Cleanuptrue,
PG exited/root removed/parent empty; same774-source manifest and HEAD/status matched.
API SHAa8461306842e34672373e195cabd4a74d6da4f085bbb4d901344d48b389ab5ff;
fixture SHA64fe9f413dd31bbb26af3065e0473bbc151ca2b29a2ffb0052c0bcaa9611b949.
Do not treat a different denial as prepared lifecycle success. Its exact400 cause
was not retained; FIX827 adds closed HTTP fingerprinting. The separately found
proof request-ID contract mismatch is recorded independently in FIX828.

## Known Limitations

The source/prepare evidence identifies a concrete claim-path failure, not proof
that it is the only remaining defect. D0.3 negatives, checkpoint recovery, native
Linux/cloud execution, true employer receipts and release gates remain open.
