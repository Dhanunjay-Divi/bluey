# FIX-819: PostgreSQL fleet cutover rejects a syntactically invalid count query

> Preflight: bluey-ops, current AGENTS and Round638; isolated private PG evidence.

## Issue

Actual API/PG reproduction62571 failed at fleet cutover begin. Independent source
inspection plus exact-query parser probe62355 confirmed PostgreSQL SQLSTATE42601.
This is a production availability/release blocker, not an authority bypass.

## Root Cause

record_runner_volume_fleet_cutover_postgres ends its unready-volume count query
with four closing parentheses for three opens: outer AND, NOT EXISTS attestation,
and nested NOT EXISTS newer attestation. SQLite's corresponding SQL is inside an
additional scalar subquery and correctly needs four. A dialect-specific boundary
was retained incorrectly in the standalone PostgreSQL statement.

## Fix Summary

Remove only the extra PostgreSQL close. Keep SQLite, every attestation predicate,
lock, readiness/expected-snapshot guard and transition unchanged. Do not bypass
cutover authority to allow the reproduction to proceed.

## Files Modified

- server/src/db/jobs/runner_volume_purge.rs: PG query syntax and one bounded actual-
  PG lifecycle/replay/genuine stale-successor regression (runtime pending).

## Edge Cases Handled

Required actual-PG coverage: reconciling→ready, exact replay, stale snapshot reject
without state mutation. Preserve existing SQLite parity; no SQLite source change.

## How to Test

The exact pre-fix965-byte production statement, SHA
6bd923ad639c3b7e20a808f22d71acc15af01667ac47c5615c5396c84e6779fc,
returned42601 under READ ONLY EXPLAIN in fresh ownedPG17.10/vector0.8.3.
Probe62355 exit0 means the intended parser rejection was observed, not source
acceptance. Actual corrected query/function/runner reproduction remains unverified.

Corrected exact964-byte query SHA
71d2f8b23eda73c80eaa238c87711812cdbddf061cd7f6590c7160440ed0596e
then passed8/8 private-PG predicate cases (90815 exit0): empty fleet, missing/current
attestation, newer stale/current generation, unreconciled tombstone, unready status,
destroyed exclusion. Only synthetic temporary tables were created/rolled back.
Query bytes remained unchanged; private PG stopped and both parser/query parents
were empty after cleanup. This is query-level evidence, not Rust-function or
nonempty ordinary API-path qualification.

Focused Rust execution2776 then passed8/8, zero skipped:1 actual-PG cutover
lifecycle+replays+genuine stale-successor immutability,2 helper model/schema tests,
4 example guards/redaction and1 existing SQLite parity.772-source manifest
4acfd0ed62eaa25eb52381b890f975bb4cf230d5cc464f5aabcc3caa7a6c8efc
matched before/after, HEAD/status unchanged; PG/root cleanup verified. Ordinary
nonempty API fixture and full source/regression gates still pending.

## Known Limitations

Run/session identity causal red and fix, positive lease lifecycle and complete
source/release gates remain pending. No deployment or external effect occurred.
