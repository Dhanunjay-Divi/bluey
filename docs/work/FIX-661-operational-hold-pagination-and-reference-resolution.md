# FIX-661: Make Redacted Hold Listings Complete and Operable

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live database, or production system
> was used.

## Issue

A bounded redacted hold list could omit later heads, while a mutation contract
that accepted only raw scope and predecessor IDs would force operators to
retain or rediscover identities the list intentionally does not expose.

## Root Cause

The initial admin surface had no stable continuation contract and treated
privacy-safe listing and mutation as unrelated operations. It could not use
the opaque `scopeRef` and `currentEventRef` returned by the API to identify the
exact current head for release or re-hold.

## Fix Summary

The list now uses encrypted keyset pagination over the stable internal
capability/scope order. Its cursor is purpose-bound, bound to `activeOnly`, and
never exposes its internal scope ID. The mutation endpoint accepts either the
strict raw request or a strict by-reference request. By-reference mutation
uses indexed head refs, recomputes and constant-time verifies both refs,
recovers the exact canonical predecessor, and then enters the same append,
compare-and-swap, and replay path as raw mutation.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/operational_holds.rs` | Add encrypted keyset pages and exact by-ref resolution/replay. |
| `server/src/api/jobs_operations.rs` | Add bounded list query/response and the strict raw-or-by-ref mutation body. |
| `infra/{sqlite,postgres}/server-runtime/*jobs_operational_holds.sql` | Add paired opaque-reference columns and lookup indexes. |
| `jobs/scripts/check-jobs-schema-parity.mjs` | Require the paired operational-hold schema and indexes. |

## Edge Cases Handled

- Forged, tampered, oversized, empty, or filter-mismatched cursors fail closed.
- `activeOnly=false` can page released heads without reusing an active cursor.
- A wrong scope ref, event ref, revision, capability, or scope kind cannot
  resolve a head.
- Ambiguous or internally inconsistent stored refs are storage corruption, not
  release authority.
- Exact by-ref replay returns the original result; changed predecessor refs
  conflict under the same event ID.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml operational_hold_listing_pages_every_head
cargo test --manifest-path server/Cargo.toml \
  opaque_refs_recover_release_authority_and_preserve_exact_replay
cargo test --manifest-path server/Cargo.toml \
  admin_routes_mutate_list_and_report_readiness_without_private_ids
node jobs/scripts/check-jobs-schema-parity.mjs
```

## Known Limitations

- Cursor encryption and keyed refs depend on the reviewed Jobs data-key
  configuration remaining consistent across the full and standalone routers.
- Live PostgreSQL indexed resolution and migration rehearsal remain external
  gates even though optional PostgreSQL coverage exists in source.
