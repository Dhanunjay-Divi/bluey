# FIX-758 — Typed operational-hold denials

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 fail-closed correctness and API privacy defect

**Status:** Implemented; focused SQLite/PostgreSQL/API evidence is green and final aggregate
evidence is pending

## Issue

Several candidate-selection and application-finalization paths flattened an active operational hold
into an ordinary storage or generic authority error. Multi-runner selection could lose the hold
after trying another candidate, and the API could not distinguish a safe conflict from internal
storage failure without inspecting text.

## Root Cause

`OperationalHoldError` did not preserve the evaluated `OperationalHoldBlock` as a typed variant at
all call boundaries. Loops that tested local and cloud routes had no structured denial to retain,
and generic API mapping risked either returning a 500 for a policy hold or exposing internal reason
details.

## Fix Summary

- Add and propagate `OperationalHoldError::Held(OperationalHoldBlock)`.
- Preserve the first typed hold while checking bounded local/cloud candidates; return it only when
  no route is authorized.
- Keep storage failures distinct and fail them as internal errors.
- Map typed holds to a redacted HTTP 409 message without scope IDs, reason references, or internal
  policy codes.
- Map browser-claim and verifier-worker holds to their closed domain outcomes without weakening
  account, employer, provider, or runner scopes.
- Prove held application finalization mutates no application, resume, reservation, browser-session,
  or run-event state in SQLite and configured PostgreSQL.

## Files Modified

| File | Change |
| --- | --- |
| `server/src/db/jobs/operational_holds.rs` | Preserve typed held evaluations. |
| `server/src/db/jobs/applications.rs` | Retain typed holds through runner-selection loops. |
| `server/src/db/jobs/original_source_verification.rs` | Map held verifier work to closed authority. |
| `server/src/db/jobs/browser_release_authority.rs` | Preserve held versus storage outcomes. |
| `server/src/db/jobs/execution_leases.rs` | Map held execution admission to conflict. |
| `server/src/api/jobs.rs` | Return redacted 409 for held and 500 for storage failure. |
| `server/src/db/jobs/tests.rs` | Add typed and zero-mutation regressions. |

## How To Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  operational_hold_domain_error_is_redacted_conflict_and_storage_stays_internal -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib \
  prepared_auto_submit_finalization_preserves_typed_employer_hold_and_mutates_nothing -- \
  --nocapture
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> cargo test --manifest-path server/Cargo.toml \
  --lib postgres_prepared_auto_submit_finalization_preserves_typed_employer_hold_when_configured \
  -- --nocapture --test-threads=1
```

## Known Limitations

- User-facing hold inspection and operator remediation remain separate reviewed product work.
- This fix does not release a hold, enable a worker, or authorize any external effect.
