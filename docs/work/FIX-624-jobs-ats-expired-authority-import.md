# FIX-624: Reject Expired ATS Authorities at Import

> **Codex preflight:** Loaded `$bluey-ops` and used the current Round 604
> authority implementation and acceptance criteria only.

## Issue

Expired signed evidence, layout observations, manifests, and activations could
be imported because their validators proved internal time ordering but did not
compare expiry with the server's import verification time.

## Root Cause

The generic signature verifier checked issue and signature timestamps, while
the four typed envelope wrappers omitted `verification_time_ms >=
expires_at_ms`. Later resolution failed expired state closed, but immutable
registry imports could still accept already-dead authority.

## Fix Summary

- Added import-time expiry checks to evidence, layout-observation, manifest,
  and activation envelope verification.
- Kept expiry decisions on server time and before any persistence mutation.
- Added focused tests covering every signed non-policy authority class.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/ats_certification_authority.rs` | Reject expired typed envelopes before signature-authorized import and cover all four classes. |
| `docs/work/FIX-624-jobs-ats-expired-authority-import.md` | Record the defect and repair. |

## Edge Cases Handled

- Exact boundary expiry (`verification_time_ms == expires_at_ms`) is expired.
- Future expiry remains subject to trust-policy and clock-skew limits.
- Byte-identical replay does not bypass current import-time expiry checks.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  expired_signed_non_policy_authorities_fail_import_closed
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- Authorized live PostgreSQL execution remains a separate parked gate.
