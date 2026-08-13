# FIX-688: Provider Refresh CAS Clock-Tick Race

> **Codex preflight:** Loaded `$bluey-ops`, inspected the failing hosted Jobs CI
> job directly, and reproduced the affected credential-authority path from the
> current Phase 608 worktree. No SSD/archive or production service was used.

## Issue

The hosted Jobs privacy/CI job intermittently failed
`stale_provider_refresh_cannot_overwrite_a_grant_upgrade`. A write-grant upgrade
and the credential snapshot it replaced could carry the same millisecond
`updated_at_ms`, allowing a stale refresh to pass the SQL timestamp predicate
and fail later with a different authority error.

## Root Cause

`save_mailbox_connection_with_credential_cas` assigned `now_ms()` before it
loaded the current mailbox and credential rows. Millisecond wall-clock time is
not guaranteed to advance between the initial grant and an immediate upgrade,
but `refresh_jobs_provider_credential_cas` uses `updated_at_ms` as part of its
compare-and-swap identity.

## Fix Summary

Both SQLite and PostgreSQL grant-upgrade transactions now derive one authority
timestamp after locking and parsing the current rows. The new value is the
maximum of wall-clock time and each stored timestamp plus one, and the exact
same value is serialized into the mailbox and credential projections. The
regression test explicitly requires the upgraded credential to advance beyond
the stale snapshot.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/customer_data.rs` | Derive a monotonic grant-upgrade CAS timestamp inside both database transactions. |
| `server/src/db/jobs/tests.rs` | Assert same-tick grant upgrades always advance refresh authority. |
| `CHANGELOG.md` | Record the hosted-CI and authority fix. |

## Edge Cases Handled

- Initial grant and consent upgrade within one millisecond.
- Mailbox and credential rows whose stored timestamps differ.
- SQLite and PostgreSQL transaction parity.
- A stale refresh after a same-tick grant upgrade.

## How to Test

```bash
cargo fmt --manifest-path server/Cargo.toml -- --check
cargo test --manifest-path server/Cargo.toml provider_refresh --lib --locked
```

The exact stale-grant regression was also run twelve consecutive times.

## Known Limitations

- Hosted CI must rerun after the fix is pushed to prove the original Linux job.
