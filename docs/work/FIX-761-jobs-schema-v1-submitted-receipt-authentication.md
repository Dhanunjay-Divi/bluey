# FIX-761 — Schema-v1 submitted-receipt authentication

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 reconciliation correctness defect

**Status:** Implemented; focused regression evidence is green and final aggregate evidence is
pending

## Issue

A valid review-first schema-1 approved packet omitted execution admission by design. Authentication
of its submitted final receipt nevertheless failed with `approved execution admission is missing`.
That broke safe finalization-error replay and precommit upload reconciliation for a legitimately
committed application.

## Root Cause

`approved_submission_snapshot` used `bool::then_some` around a fallible admission expression.
Rust evaluates the argument eagerly, so `admission.ok_or(...)?` ran even when the schema was 1 and
the Boolean was false. The intended schema-2/schema-3-only requirement therefore applied to every
schema.

## Fix Summary

- Replace eager `then_some` evaluation with an explicit schema branch.
- Pass no admission into the schema-1 checksum, preserving its closed legacy shape.
- Continue requiring exact admission for schema 2 and schema 3.
- Make the authenticated submitted-application fixture self-validate before reconciliation tests
  use it.
- Add a regression that accepts schema 1 without admission and rejects schema 2 when admission is
  removed.
- Keep complete final-envelope authentication ahead of workflow mutation and committed-receipt
  replay.

## Files Modified

| File | Change |
| --- | --- |
| `server/src/db/jobs/customer_data.rs` | Make admission validation schema-lazy and fail closed. |
| `server/src/api/jobs.rs` | Validate the submitted fixture and add schema-boundary regression. |

## Evidence

- **PASS:** finalization reconciliation authenticated receipt regression.
- **PASS:** precommit upload reconciliation authenticated fingerprint regression.
- **PASS:** schema-1 optional/schema-2 required admission regression.
- **PASS:** final frozen formerly-failing set, 16 passed and 0 failed in 20.74 seconds.
- **PENDING:** final aggregate and strict Clippy evidence.

## How To Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  finalize_error_reconciliation_requires_an_authenticated_committed_receipt -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib \
  precommit_upload_error_reconciliation_uses_authoritative_submitted_fingerprint -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib \
  submitted_receipt_authentication_keeps_schema_one_admission_optional_only -- --nocapture
```

## Known Limitations

- This fix authenticates and reconciles stored receipts; it does not send or retry an application.
- Schema 1 remains review-first only and cannot grant Auto-submit authority.
