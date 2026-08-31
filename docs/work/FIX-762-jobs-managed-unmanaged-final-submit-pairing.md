# FIX-762 — Managed/unmanaged FinalSubmit pairing

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 managed-effect authorization bypass

**Status:** Implemented; exact managed and unmanaged SQLite full-boundary regressions are green.
The configured PostgreSQL classification/replay helper has an observed green diagnostic
checkpoint, but a full PostgreSQL managed FinalSubmit fixture and final aggregate evidence remain
pending.

## Issue

The shared worker FinalSubmit boundary accepts both managed-cloud and explicitly unmanaged-cloud
execution. On PostgreSQL, omitting the complete managed authority tuple could classify a managed
workflow as the optional unmanaged case and reach the irreversible-submission transaction. SQLite
already denied the same mismatch, so the paired backends did not enforce one authoritative
managed/unmanaged classification.

## Root Cause

- The API correctly rejected partial managed tuples, but all-absent input remained ambiguous at the
  database boundary because legitimate unmanaged cloud runners also omit that tuple.
- The PostgreSQL fresh-start path did not resolve durable workflow classification before treating
  absence as unmanaged.
- Replay checked stored managed lease/receipt pairing only when the caller supplied managed input,
  so omitting input could bypass the pairing check for an already-started managed lease.
- A first attempted repair made the shared handler managed-only. Review rejected that approach
  because it would have disabled legitimate unmanaged cloud runners instead of resolving the
  ambiguity authoritatively.

## Fix Summary

- Preserve the optional tuple at the shared API and database boundary while continuing to reject a
  partial tuple as HTTP `400`.
- Resolve durable workflow classification inside both SQLite and PostgreSQL transactions before
  protected mutation. Complete managed state rejects omitted managed authority with an identity
  conflict; partial managed state rejects fail closed as invalid authority; only explicit absence
  of managed state can use the unmanaged branch.
- Always pair a replayed execution lease and receipt with the stored managed identity, even when
  the caller omitted managed input.
- Keep the dedicated managed authorization route strict and keep legitimate unmanaged execution
  available through the shared boundary.
- Add direct SQLite/PostgreSQL classification-and-replay tests plus full unmanaged and managed
  SQLite fresh/replay regressions. The managed regression covers omission, wrong-worker, exact
  success/replay, and zero-mutation denial at the real FinalSubmit boundary.

## Files Modified

| File | Change |
| --- | --- |
| `server/src/api/jobs.rs` | Preserve complete optional input at the shared boundary and reject partial tuples. |
| `server/src/db/jobs/execution_leases.rs` | Classify fresh starts and authenticate every replay before irreversible mutation. |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Add paired durable managed/unmanaged classification helpers and replay pairing evidence. |
| `server/src/db/jobs/tests.rs` | Prove legitimate unmanaged fresh execution and exact replay remain available. |

## Evidence

- **PASS:** `managed_unmanaged_submit_pairing`, 2 passed and 0 failed; the PostgreSQL case skipped
  only because that focused invocation did not provide `BLUEY_TEST_POSTGRES_URL`.
- **PASS — superseded diagnostic checkpoint:** configured PostgreSQL 17.10 managed/unmanaged
  pairing guard on `bluey_phase614b_pg17_r5`, 1 passed and 0 failed. Later source edits mean the
  helper must run again in the final frozen-source `r6` manifest.
- **PASS:** legitimate unmanaged full-boundary fresh submit and exact replay, 1 passed and 0 failed.
- **PASS:** `irreversible_submit`, 3 passed and 0 failed.
- **PASS:**
  `managed_cloud_final_submit_boundary_denies_omission_and_wrong_worker_without_mutation`, 1 passed
  and 0 failed. The exact SQLite boundary denies omitted managed input and wrong workers before
  mutation for fresh and `click_started` replay paths, admits the exact managed worker, and
  authenticates the stored proof, capacity, lease, ATS, canary, and managed receipt on replay.
- **LIMIT:** configured PostgreSQL evidence currently exercises the durable
  classification/replay-pairing helper. A production-positive full PostgreSQL managed FinalSubmit
  transaction fixture has not been constructed or run, so backend-wide transaction-boundary
  parity is not claimed.
- **PENDING:** final aggregate, strict Clippy, and exact frozen-source evidence.

## How To Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  managed_unmanaged_submit_pairing -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib \
  unmanaged_cloud_worker_submit_and_replay_accept_absent_managed_authority -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib irreversible_submit -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib \
  db::jobs::tests::managed_cloud_final_submit_boundary_denies_omission_and_wrong_worker_without_mutation \
  -- --exact --nocapture --test-threads=1
BLUEY_TEST_POSTGRES_URL=<fresh-isolated-postgres-url> cargo test \
  --manifest-path server/Cargo.toml --lib \
  db::jobs::managed_cloud_release_authority_tests::postgres_managed_unmanaged_submit_pairing_guards_are_fail_closed_when_configured \
  -- --exact --nocapture --test-threads=1
```

## Known Limitations

- This repair authorizes state transition only; it does not perform or simulate an external
  application submission.
- Local PostgreSQL does not prove hosted proxy, failover, role, backup/restore, or deployment
  behavior.
- Configured PostgreSQL currently proves durable managed/unmanaged classification and replay
  pairing only; the full managed FinalSubmit transaction/no-mutation matrix remains a bounded
  evidence gap.
- No production database, flag, provider, application, email, message, or competitor session was
  touched.
