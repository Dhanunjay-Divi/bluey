# FIX-654: Communication Diagnostics Exposed Stable Private Fingerprints

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Account export included stable hashes of private provider evidence, raw
mailbox-provider errors or URLs could carry opaque pagination tokens into logs,
and communication JSON responses lacked an explicit non-storage policy.

## Root Cause

The first export treated evidence hashes as harmless metadata and mailbox sync
logged provider error display strings instead of a small reason-code grammar.

## Fix Summary

Export only customer-meaningful communication state and payloads; remove
private evidence/reconciliation fingerprints and internal dispatch metadata.
Map provider failures to bounded coarse reason codes, redact URLs/tokens, and
add sentinels that reject credentials, scopes, grants, provider objects,
operation keys, lease data, raw errors, and stable private fingerprints.
Mark communication list/detail/mutation responses `private, no-store` with a
legacy no-cache directive, and request them with browser caching disabled.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/customer_data.rs` | Sanitized communication export projection |
| `server/src/jobs_mailbox_sync/` | Bounded reason-code logging |
| Communication API and portal client | Explicit non-storage request/response policy |
| Export/log privacy tests | Secret, token, URL, and evidence-fingerprint sentinels |

## Edge Cases Handled

- Graph skip tokens, provider URLs, OAuth tokens, scopes, grant digests,
  provider object IDs, operation keys, lease material, raw errors, and evidence
  hashes.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml customer_data --quiet
cargo test --manifest-path server/Cargo.toml mailbox_sync --quiet
node jobs/scripts/privacy-gate.mjs
```

## Known Limitations

- Production log-sink and retention certification remain external gates.
