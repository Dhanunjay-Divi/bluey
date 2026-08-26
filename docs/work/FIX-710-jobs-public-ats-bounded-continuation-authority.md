# FIX-710: Public ATS Pagination Could Continue Across Unproven Provider History

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the public SmartRecruiters and Workday
> acquisition boundary in the current Phase 613 source. All evidence is local and fetch-mocked; no
> live ATS, authenticated provider, employer contact, application, deployment, or flag was used.

## Issue

Bounded public-ATS searches could report partial results, but their first continuation contract did
not prove the complete ordered provider prefix before acquiring later rows. It also did not reject
an oversized source set before fetch or make the filter-before-history order explicit. A cursor
could therefore continue after an earlier, non-overlap row changed or carry the wrong candidate
history boundary.

## Root Cause

Offsets, advertised totals, trailing overlap, and exact dedupe/cross-listing history bounded one
continuation dimension, but trailing overlap alone could not detect a mutation earlier in the
already-consumed prefix. Full-prefix validation also needed to be resumable when the configured
`maxPages` budget was smaller than the prefix. Without a pre-fetch source cap and a strict
freshness/eligibility/query-filter boundary, hostile input or filtered rows could distort bounded
continuation state.

## Fix Summary

- Use cursor schema v2, binding the ordered complete source/query configuration, client page size,
  freshness limit, `maxPages`, current-window digest, per-source state, and exact
  dedupe/cross-listing history.
- Reject more than 24 configured sources before any fetch begins.
- Carry an incremental digest of each complete ordered provider prefix. Revalidate that prefix
  before acquiring later rows; when validation consumes the `maxPages` budget, persist its exact
  validation offset/digest and resume validation on the next bounded cursor hop.
- Continue SmartRecruiters and Workday through bounded upstream windows with exact trailing
  overlaps of 50 and 10 rows respectively. Bind the advertised total and reject prefix, overlap,
  total, cross-cut, progress, completion, or offset inconsistency.
- Apply freshness, required title/URL eligibility, and query exclusions before in-window dedupe and
  before advancing exact dedupe/cross-listing history. Positive role/location values remain server
  classification hints rather than worker filters.
- Preserve configured source order even when requests settle out of order.
- Carry seen-job and possible-cross-listing evidence across windows. Possible cross-listings remain
  separate and produce a warning for original-source comparison.
- Cap each exact-history collection and each incomplete provider prefix at 512 entries, and cap the
  encoded cursor at 192 KiB; fail closed rather than truncating authority-bearing continuation
  state.
- Keep `snapshot()` separate and fail closed on malformed rows or a pagination cap so an incomplete
  feed cannot become closure evidence.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/src/public-ats.ts` | Add bounded cursor, source overlap, exact-history, dedupe, and cross-list continuation semantics |
| `jobs/automation/tests/public-ats.test.ts` | Cover traversal, mutation detection, history bounds, cursor validation, and ordering |

## Edge Cases Handled

- more than 24 sources rejected without invoking the fetcher;
- 501 SmartRecruiters rows and 101 Workday rows across multiple acquisition windows;
- a complete prefix whose validation requires multiple `maxPages`-bounded cursor hops;
- mutation of an early consumed row outside the trailing overlap;
- duplicate and possible cross-listed postings separated by a window boundary;
- stale, invalid, or excluded candidates that must not consume dedupe/cross-list history;
- malformed, version-mismatched, checksummed-but-invalid, query-changed, stale, exhausted, or
  misaligned cursors;
- a changed advertised total, trailing overlap, or cross-cut row swap;
- an overlap-only response that makes no forward progress;
- source requests completing out of order; and
- exact-history, prefix-history, or cursor-size exhaustion.

## How to Test

```bash
(cd jobs && npm run test --workspace @bluey/jobs-automation -- \
  tests/public-ats.test.ts)
# Observed locally: 39 / 39

(cd jobs && npm run test --workspace @bluey/jobs-automation)
# Observed locally: 680 passed / 1 existing conditional skip
# Files: 37 passed / 1 skipped
```

## Known Limitations

- SmartRecruiters and Workday public feeds do not expose a stable snapshot token. Full ordered-prefix
  validation, trailing overlap, and advertised-total checks detect bounded drift while those rows
  are observed, but an unseen row can still swap after prefix validation and before acquisition.
  Cursor v2 cannot prove that an arbitrary feed stayed globally unchanged between requests. A
  rejected cursor must restart and is not evidence of a complete historical snapshot.
- Closure-authoritative `snapshot()` does not weaken this boundary: it must complete inside its
  configured cap or fail closed and preserve the last good snapshot.
- `maxPages` bounds logical provider-page operations. A logical operation may use up to the
  configured `maxAttempts` HTTP attempts (three by default), so `maxPages` is not a raw-request cap.
- The cursor checksum is unkeyed structural-consistency evidence, not a signature, authentication,
  or authorization token. The cursor is trusted-internal continuation state only; server
  classification and original-source verification remain authoritative.
- Exact-tip CI, live provider canaries, Docker/Linux, deployment, and production flags remain
  separate gates.
