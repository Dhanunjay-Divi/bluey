# FIX-700: Owner Export Could Omit Orphan Policy Evidence

> **Codex preflight:** Loaded `$bluey-ops` and reviewed owner-scoped Career Track policy export
> against the complete Phase 613 immutable ledger. No hosted database, external account, deploy,
> or production data was accessed.

## Issue

An owner export could validate every included receipt and head transition while still accepting an
orphan immutable policy revision that had no exact receipt or head-transition authority.

## Root Cause

Export validation followed receipt-to-revision and head-transition-to-revision references in one
direction. It did not prove the inverse relationship: that every exported revision appeared
exactly once in both collections and that every transition history had one terminal current head.

## Fix Summary

- Count receipts and head transitions by the exact Track, revision ID, revision number, and
  canonical-policy digest.
- Require every revision to have exactly one matching approved receipt and exactly one matching
  immutable head transition.
- Require every Track represented by head-transition history to have one current head.
- Continue reconstructing encrypted canonical JSON, predecessor chains, transition digests,
  timestamps, semantic generations, and terminal-head equality before returning the export.
- Add a schema-valid orphan-revision regression that must make export fail closed.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/taxonomy_policy.rs` | Enforce revision/receipt/head-transition bijection and terminal-head completeness during export |

## Edge Cases Handled

- a revision with neither a receipt nor a head transition;
- duplicate authority for one revision;
- a receipt or transition bound to the wrong Track or digest;
- a transition history without a current head; and
- another account's empty export without cross-tenant leakage.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  owner_export_rejects_orphan_revision_without_receipt_and_head_transition
cargo test --manifest-path server/Cargo.toml --lib canonical_track_policy_tests
```

These regressions are present in the shared tree. The configured disposable-PostgreSQL canonical
ledger regression, which exercises owner export and its complete ledger validation, passed 1/1;
the broader local PostgreSQL authority run passed 13 tests. The clean all-target Rust command
passed 1,517 tests with zero failures or ignored tests.

## Known Limitations

- Export proves database evidence and tenant scope; it does not publish, sign, or externally
  attest the archive.
- Hosted PostgreSQL and network-failure export evidence remains pending.
