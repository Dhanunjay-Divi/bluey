# FIX-641: Narrow activation could close a broader ATS circuit

> **Codex preflight:** Loaded `$bluey-ops` and verified this defect against the
> current Round 604 worktree only. No SSD/archive or external environment was
> used.

## Issue

An exact applied successor for one activation channel and target could use
`newer_activation` to close a provider, target, adapter, or runtime circuit whose
effect was broader than that successor authority.

## Root Cause

The close query matched broad circuit subject fields against one manifest but
did not prove that the successor dominated every channel and target affected by
the global circuit. A shadow `observe_only` or canary successor could therefore
clear a circuit that also fenced an older general unattended-submit head.

## Fix Summary

- Restrict automatic `newer_activation` closure to an `activation` circuit whose
  subject is the exact predecessor activation of the applied successor.
- Require the complete successor trust, manifest, activation, runtime, canary,
  revocation, quarantine, and expiry graph to remain current after database
  serialization.
- Require the existing explicitly audited `reviewed_close` transition for
  provider, target, adapter, and runtime circuits.
- Apply the same invariant in SQLite and PostgreSQL.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Restricted automatic closure to exact activation scope and added paired-dialect regressions. |
| `docs/work/FIX-636-jobs-ats-newer-activation-circuit-close.md` | Corrected the automatic-closure scope. |
| `docs/work/FIX-640-jobs-ats-current-circuit-close-authority.md` | Aligned current-authority wording with exact activation closure. |
| `docs/rounds/ROUND-604-JOBS-ATS-CERTIFICATION-AUTHORITY.md` | Required reviewed closure for broader circuits. |
| `jobs/OPERATIONS.md` | Clarified incident restoration authority. |
| `CHANGELOG.md` | Recorded the scope-widening fix. |

## Edge Cases Handled

- Shadow `observe_only` successor against a target circuit.
- Canary successor against a target circuit.
- Same-target successor against provider, target, adapter, and runtime circuits.
- Exact general activation successor against its exact predecessor activation
  circuit.
- Broader reviewed closure remains available and append-only.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  newer_activation_circuit_close -- --nocapture
```

## Known Limitations

- PostgreSQL regressions self-skip unless `BLUEY_TEST_POSTGRES_URL` names an
  authorized disposable database.
- No live circuit, activation, tenant, runner, or rollout flag was changed.
