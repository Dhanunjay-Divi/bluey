# FIX-675: Mark Mailbox Token Fixtures Explicitly Synthetic

> **Codex preflight:** Loaded `$bluey-ops` and found this issue through the
> final staged-index privacy gate in the current Phase 606 worktree. No archive,
> real credential, mailbox, provider, tenant, or production system was used.

## Issue

The privacy gate rejected four newly added mailbox-test token assignments even
though their values were synthetic.

## Root Cause

The fixture strings described their test scenario but did not contain the
privacy gate's explicit `dummy`, `test`, or `example` marker. Ambiguous
credential-shaped literals are intentionally denied.

## Fix Summary

The two Outlook fixtures now prefix both access and refresh values with
`dummy-`. Runtime behavior and test identity remain unchanged, while review and
automation can prove the values are not candidate credentials.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/tests.rs` | Mark four synthetic mailbox token values explicitly as dummy data. |

## Edge Cases Handled

- Both access and refresh token fields are explicit.
- Both held-scan fixtures use the same safe convention.
- No privacy allowlist or weaker detector was added.

## How to Test

```bash
node jobs/scripts/privacy-gate.mjs
cargo test --manifest-path server/Cargo.toml \
  communication_dispatch_hold_skips_held_candidate
cargo test --manifest-path server/Cargo.toml \
  communication_held_candidate_scan_is_bounded
```

## Known Limitations

- This is source-fixture evidence only; it does not exercise a provider token.
