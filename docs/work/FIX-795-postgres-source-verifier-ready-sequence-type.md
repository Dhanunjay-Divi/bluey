# FIX-795: PostgreSQL boot-ready sequence parameter inference

> Codex preflight: `bluey-ops`, current Jobs worktree and Round 629.

## Issue

The first configured PostgreSQL startup/concurrency regression failed before
readiness committed: `cannot convert between the Rust type i64 and the Postgres
type int4`. SQLite and unconfigured PostgreSQL tests did not expose this boundary.

## Root Cause

`commit_source_verifier_boot_ready_postgres_tx` used the same uncast `$3`
parameter in `SET heartbeat_sequence=$3` and `heartbeat_sequence=$3-1`.
PostgreSQL resolved the arithmetic with integer literal `1` as `int4`; the Rust
driver correctly refused to serialize the `i64` sequence into that parameter.
The established generic heartbeat UPDATE already casts this arithmetic to
`BIGINT`, but the boot-specific successor had omitted that type constraint.

## Fix Summary

Cast both sequence uses to `BIGINT`, preserving the exact predecessor, degraded
startup state, transaction, lock order and one-row update requirement. Extract
that exact UPDATE into a private SQL constant so the configured regression can
prepare the production statement rather than an independently copied query.
No migration, schema, sequence limit or readiness fence is loosened.

## Files Modified

| File | Change |
|---|---|
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Explicit PostgreSQL sequence types and configured driver-boundary regression |

## Edge Cases Handled

- Assert PostgreSQL prepares parameter types as TEXT, INT8, INT8, INT8.
- Bind a real `i64` value above `i32::MAX` against a guaranteed-missing runtime
  and require zero updated rows, without manufacturing authority state.
- Continue into actual concurrent startup, canonical replay, changed-payload
  rejection, replacement boot and stale-heartbeat fencing.

## How to Test

Use the canonical isolated launcher and the task-owned PostgreSQL harness with
installed PostgreSQL 17/pgvector; inject no owner or production database URL.
Run the configured
`postgres_original_source_verifier_startup_is_atomic_and_replays_under_concurrency_when_configured`
test with the harness's require-PostgreSQL flag. The pre-fix execution failed
with exit 101 after the migration regression passed. The final configured batch
passed this real driver-boundary/concurrency case among 30/30 isolated cases;
strict all-target Clippy also passed. Full-suite closeout is recorded in the
Phase 629 IMPL.

## Known Limitations

This correction does not prove hosted database behavior, runner capacity,
signed activation, real employer submission or deployment readiness. Those
remain separate gates; no production action is authorized by this fix.
