# FIX-020: Fail-Safe Calendar Token Cleanup

## Issue

A secure calendar token bundle could coexist indefinitely with legacy plaintext
or split-keychain credentials. Disconnect also stopped at the first deletion
error, allowing a surviving legacy credential to be migrated back later.

## Root Cause

Bundle loads returned before retrying legacy cleanup, cleanup loops used `?` on
the first failure, and disconnect deleted the authoritative bundle before all
legacy stores were known to be clear.

## Fix Summary

Secure saves and bundle loads now retry cleanup of all four legacy keychain
entries and the plaintext development file. Cleanup attempts every legacy target
and returns one redacted aggregate error. Disconnect clears legacy locations
first and deletes the secure bundle only after they all succeed, preventing a
partial failure from resurrecting a stale account.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-calendar-cloud/src/tokens.rs` | Added exhaustive cleanup, safe deletion ordering, retry-on-load, and failure tests. |
| `CHANGELOG.md` | Recorded the credential cleanup hardening. |

## Edge Cases Handled

- Multiple legacy keychain deletions fail in one cleanup attempt.
- The plaintext file cleanup fails after a secure bundle was saved.
- A prior partial migration left a secure bundle plus legacy credentials.
- Disconnect cleanup fails before the secure bundle is removed.
- Cleanup diagnostics include locations only and never token values.

## How to Test

```bash
cargo test -p cue-calendar-cloud tokens::tests
```

## Known Limitations

- OS keychain and filesystem failures still require the underlying permission or
  lock problem to be corrected; the operation now fails safely and retries on a
  later load/save/disconnect.
