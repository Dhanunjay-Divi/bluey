# FIX-772: Public-beta audit actors were trusted as arbitrary strings

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The database mutation API accepted any non-empty actor identifier and hashed it
into an administrator audit record. HTTP routing authenticated administrators,
but the lower authority layer could not prove the audit actor existed or still
had administrator status.

## Root Cause

`AdminAuditContext::Actor` carried a string rather than transactionally
validated account authority. Test fixtures therefore succeeded with a
synthetic nonexistent actor.

## Fix Summary

Every audited cohort, grant, and override mutation now verifies in the same
transaction that the actor account exists, is currently an administrator, and
has no deletion intent. The integration harness seeds its dedicated audit
actor as a verified administrator before its audited cohort setup.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs_beta_access.rs` | Validate the current administrator within SQLite/PostgreSQL transactions |
| `server/tests/integration_e2e.rs` | Seed the exact harness actor as a real administrator |
| `jobs/scripts/ci-guards-self-test.mjs` | Preserve the typed audited production mutation boundary |
| `docs/work/FIX-772-public-beta-real-admin-audit-actor.md` | Record diagnosis and verification |

## Edge Cases Handled

- Missing, non-administrator, demoted, or deletion-pending actors fail closed.
- Mutation and redacted audit insertion remain one transaction.
- Private test-only unaudited helpers remain unavailable to production callers.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  jobs_beta_access::tests::sqlite_audited_mutations_require_a_current_admin_actor
node jobs/scripts/ci-guards-self-test.mjs
```

## Known Limitations

- Exact Rust and configured PostgreSQL execution remain pending on the final
  integrated tip.
