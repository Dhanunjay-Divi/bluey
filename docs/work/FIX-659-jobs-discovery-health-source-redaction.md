# FIX-659: Redact Discovery Sources and Isolate Diagnostic Secrets

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive or production system was used.

## Issue

The discovery health script printed each overdue `source_key` verbatim. Its
helper processes could also inherit unrelated Bluey/provider secrets, and the
database URL could have become observable in child process arguments. Those
values are unnecessary for a service-health alert.

## Root Cause

`ops/check-bluey-jobs-discovery.sh` concatenated the source kind, provider, and
raw database `source_key` into stderr. It also relied on the ambient process
environment when invoking diagnostic helpers instead of defining an explicit,
minimal child boundary.

## Fix Summary

The query now emits only a hex-encoded canonical JSON tuple into a local pipe.
A local helper validates a dedicated 32-byte diagnostic key and emits a
deterministic `ref-` prefix followed by a domain-separated HMAC-SHA-256 digest.
Kind and provider remain available for routing the incident; the source key
itself never reaches the alert boundary. The Jobs data-encryption key is not
reused, and neither key is sent in SQL or process arguments.

The script resolves exact `psql` and Python executable paths, starts helpers
with a clean environment, and passes the database URL or diagnostic key through
an inherited file descriptor. The `psql` child receives only a bounded
allowlist including `PGDATABASE`; the renderer rejects any inherited database,
diagnostic, data-encryption, or provider key. Diagnostic kind/provider values
also come from a closed set before they can reach alert output.

## Files Modified

| File | Change |
|------|--------|
| `ops/check-bluey-jobs-discovery.sh` | Replace raw source output, sanitize helper environments, and pass secrets through file descriptors. |
| `ops/tests/test-check-bluey-jobs-discovery.sh` | Require opaque output, clean child environments, argv secrecy, and closed diagnostic dimensions. |
| `jobs/OPERATIONS.md` | Document the dedicated key, Python 3 dependency, bounded alert form, process boundary, and external provisioning gate. |

## Edge Cases Handled

- Direct and global sources use the same opaque-reference construction.
- Alert ordering remains deterministic without printing the ordered key.
- Canonical JSON prevents ambiguous field concatenation.
- PostgreSQL statement logs contain neither secret key nor raw source value.
- The database URL does not appear in `psql` argv.
- Data-encryption and provider keys in the parent environment do not reach
  either diagnostic helper.
- Unknown or newline-bearing kind/provider dimensions fail closed without
  echoing attacker-controlled values.
- The local helper rejects malformed keys and diagnostic rows.
- No PostgreSQL extension is required.

## How to Test

```bash
bash ops/tests/test-check-bluey-jobs-discovery.sh
```

## Known Limitations

- The reference is for log correlation only and is not authentication or
  cryptographic authority.
- Production must provision `BLUEY_JOBS_DIAGNOSTIC_KEY` consistently wherever
  this health check runs; Python 3 is a required local dependency.
- No production secret-manager, timer host, process-table inspection, or live
  PostgreSQL execution was available for this source-only fix.
