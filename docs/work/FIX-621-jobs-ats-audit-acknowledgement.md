# FIX-621: ATS authority writes could be acknowledged without an operations audit

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

ATS certification handlers returned successful mutation responses even when
the redacted operations-audit insert failed.

## Root Cause

The authority transaction recorded the exact actor on its immutable database
row, but the API-layer `record_audit` helper logged operations-audit failures
and returned `()` instead of propagating them.

## Fix Summary

Every ATS authority handler now requires its redacted operations-audit insert
to succeed before acknowledging the mutation. A transient failure returns a
generic internal error; the signed byte-identical replay path can safely retry
and complete the audit without creating or widening authority.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs_ats_certifications.rs` | Makes all admin and worker mutation audit helpers fallible and propagates audit failure before the HTTP success response. |

## Edge Cases Handled

- Audit error details and actor identities remain absent from public responses.
- Exact authority replay remains the only repair path after a transient audit
  failure.
- Mutation metadata remains digest-only and does not expose evidence paths,
  target identities, account lists, or signing material.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib api::jobs_ats_certifications::tests
cargo test --manifest-path server/Cargo.toml --test integration_e2e \
  ats_certification_routes_isolate_admins_workers_paths_and_bodies -- --exact
```

## Known Limitations

- The immutable authority row and redacted operations event are separate
  records. A failed audit response may leave an unacknowledged authority row,
  but no client receives success until an exact replay durably records the
  operations event.
