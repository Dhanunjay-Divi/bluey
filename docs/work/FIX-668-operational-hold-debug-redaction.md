# FIX-668: Redact Private Hold Identity From Debug and Error Chains

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live tenant, log platform, or
> production system was used.

## Issue

Operational-hold HTTP projections were redacted, but derived Rust `Debug`
output could still expose raw scope IDs, event IDs, reason references, operator
identity, or every exact value in an admission context when a result or error
chain was logged during incident handling.

## Root Cause

`OperationalHoldState`, `OperationalHoldBlock`, and `OperationalHoldContext`
contained private authority fields and relied on automatically derived
`Debug`. Serialization controls such as `skip_serializing` do not affect Rust
debug formatting or nested enum/error formatting.

## Fix Summary

The three private types now implement explicit redacted `Debug` projections.
State and block output preserve only bounded operational dimensions while
replacing private values with redaction markers. Context output contains only
the closed scope-kind names and value counts. Nested
`OperationalCapabilityEvaluation::Held` and `OperationalHoldError::Held`
therefore inherit the same safe projection.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/operational_holds.rs` | Replace derived private `Debug` output with explicit redacted projections and regression coverage. |

## Edge Cases Handled

- Raw account, Career Track, source, employer, model, and other scope values do
  not appear in state, block, context, evaluation, or held-error debug output.
- Raw event ID, operator identity, and optional reason reference remain absent.
- Context remains useful for diagnosis through closed scope-kind counts without
  revealing the values being evaluated.
- Public JSON redaction remains independent and unchanged.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  debug_projections_redact_private_operational_hold_identity
cargo test --manifest-path server/Cargo.toml \
  public_state_redacts_scope_reason_and_actor
```

## Known Limitations

- This fix controls these Rust debug projections; it does not replace the
  deployment log-scrubbing, retention, and access-control gates.
- Production log ingestion and incident-response consoles were not exercised.
