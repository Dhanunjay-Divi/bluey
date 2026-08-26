# FIX-702: Unapproved Track Review Reasons Were Not Durably Persisted

> **Codex preflight:** Loaded `$bluey-ops` and traced Career Track normalization, policy review,
> persistence, and read-back in the current Phase 613 worktree. No external account, source,
> deployment, or production flag was used.

## Issue

Creating a Track without the identity or source-resume inputs required for approval could return
specific `needs_review` reasons while storing an earlier payload that omitted those reasons.

## Root Cause

Track upsert wrote a staging JSON projection before canonical policy preparation populated the
unapproved authority. The final authoritative projection write was coupled to the successful
policy-record branch, so the `None`/unapproved path could leave the pre-review payload in storage.

## Fix Summary

- Persist the final normalized Track projection after canonical policy preparation in both
  approval and unapproved branches.
- Keep `review_state=needs_review` and the exact deduplicated reason codes returned by policy
  preparation.
- Require the final tenant-owned update to affect exactly one row.
- Add a regression that compares the returned, raw stored, and listed reason codes.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/profile_postings.rs` | Persist the final authority projection for approved and unapproved Tracks and test exact read-back |

## Edge Cases Handled

- missing verified application identity;
- missing current source resume;
- more than one simultaneous review reason;
- encrypted raw-row read-back; and
- list projection after the write transaction commits.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  unapproved_track_reason_codes_are_persisted
```

The regression exists in the current tree. A configured local PostgreSQL authority suite passed
13 tests, but no dedicated PostgreSQL reason-persistence result is attributed to this fix. The
named regression is included in the clean 1,401-test server-library target, which passed; the full
all-target result was 1,517 passed with zero failures or ignored tests.

## Known Limitations

- Review reasons explain why authority was not minted; they do not themselves authorize queueing
  or Auto-submit.
- Portal presentation remains defense in depth over the server projection.
