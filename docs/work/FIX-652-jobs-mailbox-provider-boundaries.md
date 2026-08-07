# FIX-652: Mailbox Source Identity Was Not Exact Enough For Replies

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Reply delivery could target an inferred sender instead of one exact RFC
`Reply-To`, provider message identifiers were not fully connection-scoped, and
unbounded provider JSON parsing could accept an oversized mailbox response.

## Root Cause

Mailbox normalization predated write authority. It preserved enough metadata
for read-only display but not every exact reply-target, provider-object,
connection, legacy-enrichment, and response-size invariant required before an
employer-facing write.

## Fix Summary

Parse exactly zero or one valid RFC `Reply-To` and otherwise fall back to the
single validated sender; reject ambiguous targets; bind every provider object
and message hash to its connection; safely enrich legacy exact metadata; and
use bounded response-body/JSON parsing for Gmail and Microsoft mailbox sync.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/mailbox_sync.rs` | Connection-scoped source identity and legacy enrichment |
| `server/src/jobs_mailbox_sync/{mod,providers}.rs` | Exact header parsing and bounded provider responses |
| Mailbox sync tests | Reply target, collision, legacy, and oversized fixtures |

## Edge Cases Handled

- Multiple or malformed Reply-To values, From fallback, provider-ID collision
  across two connections, legacy rows, oversized bodies, and mismatched reply
  payload recipients.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml jobs_mailbox_sync --quiet
cargo test --manifest-path server/Cargo.toml mailbox_sync --quiet
```

## Known Limitations

- Live mailbox-format diversity remains an authorized provider-canary gate.
