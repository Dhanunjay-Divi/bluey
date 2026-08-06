# FIX-622: Bound ATS Phase-B Quarantine Outcome Size

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the active
> Round 604 worktree and strict server gate. No archive or external system was
> used.

## Issue

The new safety-only Phase-B outcome made both the shared ATS transaction enum
and the local-runner adapter enum more than 500 bytes because their success
variants embedded the complete certified receipt authority by value. Strict
Clippy rejected both `large_enum_variant` regressions.

## Root Cause

`AtsCertificationPhaseBTransactionOutcome::Authorized` and
`LocalAtsCertificationConsume::Authorized` paired a large success payload with
a fieldless `LayoutDriftQuarantined` variant. Rust therefore sized every enum
instance for the large authority object even on the denial path.

## Fix Summary

- Boxed the large success payload in both enums.
- Preserved ownership and exact receipt bytes by moving the boxed value back
  into the existing return type at the local transaction boundary.
- Left the fieldless committed-denial outcome and all transaction ordering
  unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Box the successful Phase-B result and unwrap it only at the legacy result boundary. |
| `server/src/db/jobs/local_runner.rs` | Box the local certified receipt outcome and move it into the final authorization record. |
| `docs/work/FIX-622-jobs-ats-quarantine-outcome-size.md` | Record the strict-lint regression and repair. |

## Edge Cases Handled

- The quarantine-only variant remains allocation-free.
- No serialized wire shape changes because these enums are internal
  transaction outcomes.
- Receipt authority is still moved exactly once and is never cloned or
  reconstructed.

## How to Test

```bash
cargo fmt --manifest-path server/Cargo.toml --all -- --check
cargo check --manifest-path server/Cargo.toml --tests
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml
```

## Known Limitations

- None. This is an internal representation correction; the separate live
  PostgreSQL and provider/device gates remain unchanged.
