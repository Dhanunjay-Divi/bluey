# FIX-755 — Jobs submission-capacity post-lock database clocks

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B
> and the current authoritative worktree. The SSD archive was not used.

**Severity:** P1 production-boundary correctness defect

**Status:** Implemented in source; final focused, configured-PostgreSQL, and aggregate evidence
pending

## Issue

Cloud and local FinalSubmit paths could reject an otherwise valid submission-evidence reservation
when the request capacity used process wall time but the final transaction used database time. A
one-millisecond clock-domain or rounding difference was sufficient for the request's `now_ms` to
appear later than the authoritative database time and fail closed as `InvalidRequest`.

Separately, PostgreSQL application-evidence upload reservation could wait on its application,
upload, outbox, or exact capacity rows and then validate capacity expiry, stamp the upload/outbox,
and account daily usage using caller-supplied pre-wait time. Capacity that expired during the wait
could therefore be consumed as if it were still current.

## Root Cause

The capacity request was created before the transaction's final blocking locks. SQLite derives its
authoritative millisecond scalar through `julianday`, while callers use the process clock. Those
clocks can differ slightly, and a transaction can also wait after the request is constructed.

FinalSubmit correctly revalidated signed source, ATS, integrity, runner, ticket or lease, session,
and capacity authority at the final locked database time, but its capacity value still retained a
request-clock window. The PostgreSQL cloud path had a partial local clone for `now_ms`; the local
path likewise replaced only `now_ms`. Neither consistently rebuilt the complete bounded capacity
window from the authoritative scalar.

The application-object path accepted `input.now_ms` as ordinary object metadata and propagated it
through capacity validation and effect timestamps. Its exact capacity lock was not followed by a
fresh database-time sample, so row-lock waiting could make the temporal decision stale.

## Fix Summary

- Cloud FinalSubmit clones the capacity only after the final effect locks and rebases both
  `now_ms` and `expires_at_ms` to the final database scalar and the fixed reconciliation grace.
- Local FinalSubmit applies the same helper contract for initial and click-started authorization.
- Both helpers use saturating addition for the fixed grace bound.
- A focused regression proves cloud and local helpers produce the exact same database-time window.
- PostgreSQL application-evidence reservation locks the application, existing upload, PUT outbox,
  and exact capacity row before sampling `clock_timestamp()` once.
- That post-lock scalar drives capacity validation/CAS, object and outbox timestamps, daily usage,
  and replay; expired object metadata is rejected before mutation.
- A configured PostgreSQL contention regression holds the capacity row past expiry and proves the
  waiting reservation mutates neither capacity, upload, outbox, nor daily usage.
- Existing exact signed authority, proof binding, capacity reservation, and zero-effect denial
  checks remain mandatory; the repair does not make request time authoritative or widen any
  production flag.

## Files Modified

| File | Change |
| ---- | ------ |
| `server/src/db/jobs/execution_leases.rs` | Rebase cloud FinalSubmit capacity to the final locked database-time window. |
| `server/src/db/jobs/local_runner.rs` | Rebase local FinalSubmit capacity to the same database-time window. |
| `server/src/db/object_uploads.rs` | Sample PostgreSQL application-upload time only after every effect row is locked. |
| `server/src/db/jobs/tests.rs` | Add exact cloud/local capacity-window regression coverage. |
| `docs/work/FIX-755-jobs-final-submit-database-clock-window.md` | Record defect, repair, and evidence contract. |

## Edge Cases Handled

- Process time is one or more milliseconds ahead of SQLite's database scalar.
- A transaction waits on authority or effect rows after the request capacity was constructed.
- An application upload waits past exact capacity expiry and then fails with zero effect mutation.
- Existing-upload replay uses the same exact capacity lock and post-lock database-time decision.
- Cloud and local paths cannot drift to different capacity-window semantics.
- Expiry arithmetic saturates instead of overflowing near the maximum supported integer.
- Rebased time cannot bypass signed authority, lease/ticket, browser-session, proof, or capacity
  validation.

## Evidence

- **SUPERSEDED diagnostic checkpoint:** an earlier local PostgreSQL 17 `r5` run of
  `postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing` passed. Later source
  edits superseded that binary, so the configured case remains pending in the final fresh
  frozen-source manifest.
- **PENDING:** final exact-tip focused, configured-PostgreSQL, aggregate, and strict-Clippy gates.

## How to Test

```bash
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  submission_evidence_capacity_uses_the_final_database_time_window -- --nocapture

CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  final_submission_schema_two_certified_envelope_is_closed_and_exact -- --nocapture

CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_application_object_upload_uses_one_post_lock_database_clock -- --nocapture

BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib \
  postgres_application_upload_waiting_past_capacity_expiry_mutates_nothing -- \
  --nocapture --test-threads=1

BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib postgres_ -- \
  --nocapture --test-threads=1

CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml
CARGO_INCREMENTAL=0 cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- The local PostgreSQL lane is disposable development evidence, not hosted PostgreSQL failover,
  interruption, or production-clock evidence.
- This fix does not deploy, enable Browser distribution, authorize an external submit, or alter a
  production flag.
