# FIX-774: Account export omitted public-beta admission and denial data

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

Public-beta enrollment and override rows were absent from the downloadable
Jobs account export. An admitted account that never created a Jobs profile
received no Jobs export at all.

## Root Cause

`JobsAccountExport` predated the public-beta tables, and `account_export`
treated a `jobs_profiles` row as the only proof that account-owned Jobs data
existed.

## Fix Summary

The export now includes only the requesting account's enrollment cohort,
source, admission time, denial state, override revision, and timestamps. It
does not expose cohort capacity, aggregate counts, release identity, or another
account. `workspace` is optional so beta-only accounts serialize it as `null`;
existing profile-backed exports serialize the same workspace object as before.
Exporting beta-only or deletion-pending data does not synthesize a profile or
application identity.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs_beta_access.rs` | Add account-scoped paired-backend export queries and tests |
| `server/src/db/jobs.rs` | Add optional workspace and bounded beta export fields |
| `server/src/db/jobs/workspace.rs` | Recognize beta-only data and avoid write-producing workspace setup |
| `server/src/db/jobs/tests.rs` | Preserve existing profile-backed export assertions |
| `docs/work/FIX-774-public-beta-account-export-completeness.md` | Record diagnosis and verification |

## Edge Cases Handled

- Admitted but never-onboarded accounts receive a structured export.
- A denied override and deletion-pending state remain exportable.
- Empty beta arrays are omitted for legacy profile-only exports.
- Existing workspace JSON remains an object when present.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  jobs_beta_access::tests::sqlite_deletion_pending_beta_only_export_is_complete_and_read_only
BLUEY_TEST_POSTGRES_URL=postgres://isolated-test-only \
  cargo test --manifest-path server/Cargo.toml \
  jobs_beta_access::tests::configured_postgres_beta_only_export_is_complete_and_read_only
```

## Known Limitations

- The configured PostgreSQL exercise exists but was not run locally because no
  isolated test database was configured.
