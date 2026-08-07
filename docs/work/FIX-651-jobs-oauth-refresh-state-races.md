# FIX-651: OAuth State And Refresh Races Could Preserve Stale Authority

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Concurrent reauthorization, token refresh, grant change, or callback error
could leave replayable OAuth state, overwrite a newer refresh-token rotation,
or retain write authority that no longer matched the bound account connection.

## Root Cause

The initial flow did not consume error callbacks through the same state fence,
and token/grant persistence did not uniformly use account-first serialization,
compare-and-swap revision checks, and one conservative size contract.

## Fix Summary

Consume and bind OAuth state before processing success or provider-declined
callbacks; serialize account/connection reauthorization; persist access,
refresh, and grant revisions atomically; use compare-and-swap refresh rotation
so a loser adopts the winner; fail closed on invalid grants; and enforce one
exact bounded token limit from callback through later refresh.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs_mailbox_oauth.rs` | State-first callback and connection fences |
| `server/src/jobs_provider_auth.rs` | Bounded token/grant derivation and refresh CAS |
| Mailbox OAuth/auth database tests | Replay, rotation, downgrade, and reauth races |

## Edge Cases Handled

- Declined-consent replay, error-callback replay, concurrent refresh-token
  rotation, omitted refresh scope, explicit scope downgrade, revoked grant,
  account deletion, and over-limit provider tokens.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml jobs_mailbox_oauth --quiet
cargo test --manifest-path server/Cargo.toml jobs_provider_auth --quiet
```

## Known Limitations

- Approved production OAuth applications and provider consent reviews remain
  external gates.
