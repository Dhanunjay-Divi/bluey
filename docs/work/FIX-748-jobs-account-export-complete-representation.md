# FIX-748: Jobs Account Export Complete Representation

**Severity:** P2 privacy-export availability

**Status:** Implemented; focused evidence green; independent review pending

## Issue

`account_export` reused the UI workspace's explicit 500-posting representation bound. An account
with more than 500 total postings therefore received HTTP 500 instead of a complete privacy export.
The materialization cap limits one discovery publication, not all historical/user-imported rows.

## Required Fix

- Export every tenant-owned posting deterministically without returning raw mutable positive
  authority labels.
- Process postings and authority rows in bounded keyset-ordered batches.
- Preserve the UI list's explicit nontruncating 500-row pagination requirement.
- Add 501+ row export completeness, tenant isolation, sanitization, and deletion-path regressions.

## Evidence

The 501-row SQLite regression proves complete deterministic order, tenant isolation, mutable-label
sanitization, embedded-eligibility removal, and preservation of a closed hard denial. The four-test
workspace representation slice and server check pass. Aggregate gates, a 501-row live PostgreSQL
export case, and post-fix independent review remain pending.
