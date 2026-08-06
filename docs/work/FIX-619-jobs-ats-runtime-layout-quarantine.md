# FIX-619: Preserve ATS Runtime Layout Quarantine Before Submit

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the Phase
> 604 plan, current branch, migrations, and Phase B call sites before diagnosis
> or implementation.

## Issue

An unknown ATS layout at Phase B returned `ScopeMismatch`, so the surrounding
transaction rolled back without preserving quarantine evidence or opening a
safety circuit. The denial prevented the immediate click, but the same runtime
remained eligible for later work and the drift was not auditable.

## Root Cause

`sqlite_ats_phase_b_observed_surface` and
`postgres_ats_phase_b_observed_surface` returned a normal error when no signed
manifest observation matched. Phase B is embedded in the local ticket and
cloud lease transactions, where any returned error rolls back every write.
There was no safety-only transaction outcome that callers could commit while
still refusing the irreversible marker.

## Fix Summary

- Added a paired SQLite/PostgreSQL append-only runtime-layout quarantine ledger.
  Evidence contains only bounded certification hashes, a bounded variant token,
  and layout contract metadata; no account, application, candidate value, page
  body, URL, cookie, token, document, or screenshot is stored.
- Deduplicated exact structural drift and capped exact evidence at 64 rows for
  an activation/runtime. Further layouts converge on one immutable overflow
  record, bounding adversarial cardinality.
- Unknown layouts atomically insert or replay quarantine evidence and open or
  hold the exact runtime circuit. Runtime code can only create `opened`/`held`
  layout-drift transitions; reviewed circuit closure remains administrator-only.
- Added an explicit `LayoutDriftQuarantined` Phase B transaction outcome.
  Local and cloud callers commit that safety-only outcome and return denial.
  Final-submit proof binding, evidence-capacity reservation, certification
  binding consume, canary reservation, and click-marker writes remain after the
  outcome and therefore do not execute on drift.

## Files Modified

| File | Change |
|------|--------|
| `infra/sqlite/server-runtime/048_jobs_ats_certification_authority.sql` | Added bounded append-only runtime-layout quarantine evidence and immutability triggers. |
| `infra/postgres/server-runtime/026_jobs_ats_certification_authority.sql` | Added the dialect-equivalent evidence ledger, index, and immutability triggers. |
| `server/src/db/jobs/ats_certification_authority.rs` | Added evidence/circuit persistence, committed-denial transaction outcome, dialect implementations, and focused tests. |
| `server/src/db/jobs/local_runner.rs` | Commits safety-only drift denials before returning and orders irreversible writes afterward. |
| `server/src/db/jobs/execution_leases.rs` | Applies the same committed-denial boundary to cloud leases. |
| `server/src/db/jobs/tests.rs` | Guards SQLite/PostgreSQL local/cloud marker, proof, and capacity ordering. |
| `docs/work/FIX-619-jobs-ats-runtime-layout-quarantine.md` | Documents the defect, fix, and verification. |

## Edge Cases Handled

- Exact replay does not append duplicate evidence or duplicate circuit events.
- A new structural layout while the circuit is open appends evidence and moves
  the circuit to `held` without widening its scope beyond the exact runtime.
- A repeated drift after reviewed closure reopens the runtime circuit.
- Layout contract-version mismatch follows the same quarantine path as a new
  variant or surface digest.
- Evidence remains immutable under attempted update or deletion.
- Exhausting the exact-evidence bound preserves one overflow record and keeps
  the circuit held without unbounded row growth.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  phase_b_surface_mismatch_commits_quarantine_and_circuit_only -- --nocapture

cargo test --manifest-path server/Cargo.toml \
  phase_b_runtime_layout_quarantine_is_append_only_and_bounded -- --nocapture

cargo test --manifest-path server/Cargo.toml \
  layout_drift_denial_commits_before_irreversible_submit_writes -- --nocapture

BLUEY_TEST_POSTGRES_URL=... cargo test --manifest-path server/Cargo.toml \
  postgres_phase_b_surface_mismatch_commits_quarantine_and_circuit_only \
  -- --nocapture

node jobs/scripts/check-jobs-schema-parity.mjs
cargo fmt --manifest-path server/Cargo.toml --all -- --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
git diff --check
```

The three local focused tests passed. The PostgreSQL test compiles and is
conditionally skipped when `BLUEY_TEST_POSTGRES_URL` is unavailable; a live
PostgreSQL execution remains an external-environment gate.

## Known Limitations

- Live PostgreSQL behavior was not exercised because no authorized
  `BLUEY_TEST_POSTGRES_URL` was available in this environment.
- Circuit closure still requires the existing administrator-authenticated,
  reviewed-close path; this fix intentionally grants no runner or client close
  capability.
