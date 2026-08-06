# FIX-647: Read-Only OAuth Could Not Authorize Provider Writes

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Existing Gmail and Microsoft grants were intentionally read-only, but the queue
had no explicit write-consent revision or exact returned-scope authority for
replies and calendar events.

## Root Cause

OAuth configuration stored scopes but advertised fixed read capabilities and
had no connection-bound communication-upgrade purpose or grant digest.

## Fix Summary

Keep read-only consent unchanged, add a separately flagged connection-bound
write-consent flow, derive capabilities only from provider-returned grants,
persist a monotonic grant revision/digest, keep refresh credentials server-only,
and invalidate dispatch authority on grant rotation or revocation.

## Files Modified

| File | Change |
|------|--------|
| `server/src/jobs_provider_auth.rs` | Server-only config, scopes, refresh, grant digest |
| `server/src/api/jobs_mailbox_oauth.rs` | Explicit bound write-consent flow |
| Mailbox sync/auth tests | Rotation, missing-scope, and revocation coverage |

## Edge Cases Handled

- Flag revoked mid-consent, different provider subject, connection changed,
  missing scope response for writes, refresh-token rotation, invalid_grant.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml jobs_provider_auth --quiet
cargo test --manifest-path server/Cargo.toml jobs_mailbox_oauth --quiet
```

## Known Limitations

- Approved Google/Microsoft applications and live consent remain external.
