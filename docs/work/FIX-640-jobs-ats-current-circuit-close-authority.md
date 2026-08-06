# FIX-640: Newer-activation circuit close accepted stale successor authority

> **Codex preflight:** Loaded `$bluey-ops` and verified this defect against the
> current Round 604 worktree only. No SSD/archive or external environment was
> used.

## Issue

A `newer_activation` circuit-close event could name an exact applied successor
but backdate its event time to before that successor was revoked, expired,
quarantined, or otherwise lost its runtime or canary authority.

## Root Cause

The SQLite and PostgreSQL close validators proved successor ordering and scope
at the caller-supplied event time. They did not re-evaluate the complete
successor authority graph at the server-controlled transaction time.

## Fix Summary

- Revalidate current trust, activation, manifest, base revocation, quarantine,
  runtime, and canary-allowlist authority at `recorded_at_ms`.
- Sample `recorded_at_ms` only after acquiring the SQLite write transaction or
  PostgreSQL authority lock, so a serialized revocation cannot be bypassed by a
  stale request-arrival timestamp.
- Ignore circuit state only inside the runtime-availability predicate while the
  compare-and-swap transition closes the exact requested head; retain runtime
  revocation and quarantine checks.
- Require every runtime bound to the exact successor activation to remain
  available.
- Preserve the existing exact applied-successor, transition ordering, and scope
  checks at the event time.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Added paired-dialect current-authority validation and adversarial SQLite/PostgreSQL coverage. |
| `docs/work/FIX-636-jobs-ats-newer-activation-circuit-close.md` | Clarified the current transaction-time restoration requirement. |
| `docs/rounds/ROUND-604-JOBS-ATS-CERTIFICATION-AUTHORITY.md` | Bound newer-activation restoration to server transaction time. |
| `jobs/OPERATIONS.md` | Documented the fail-closed incident ceremony. |
| `CHANGELOG.md` | Recorded the hardened restoration invariant. |

## Edge Cases Handled

- Backdated close after successor activation revocation.
- Backdated close after exact runtime revocation.
- Close received exactly at successor expiry.
- Backdated canary close after allowlist revocation.
- Revocation winning database serialization before a close.
- Exact activation circuit closure is not rejected solely because broader
  resolution still observes another open circuit.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  newer_activation_circuit_close_requires_current -- --nocapture
cargo test --manifest-path server/Cargo.toml \
  postgres_newer_activation_circuit_close -- --nocapture
```

## Known Limitations

- The PostgreSQL regression compiles locally but self-skips unless
  `BLUEY_TEST_POSTGRES_URL` names an authorized disposable database.
- No live circuit, activation, tenant, runner, or rollout flag was changed.
