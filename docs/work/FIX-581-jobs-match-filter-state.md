# FIX-581: Jobs Match Filter State And Authority Parity

## Issue

Bluey Jobs match filters looked functional but were local component state.
Refresh, browser navigation, and direct links discarded them. Several edge
cases also produced misleading results: inactive Career Tracks could leak into
counts, 100% matches could not be selected with the score control, workplace
matching used substrings, and a filtered-empty view looked like an account with
no discovery data.

## Root Cause

`jobs/portal/src/views/MatchesView.tsx` owned query, track, score, workplace,
packet, outside-rule, passed-job, and row-density state independently. There
was no typed URL contract or shared normalization function. Filtering also
operated on all workspace tracks and used ad hoc string matching.

The server eligibility authority was already stricter than this view. The
portal needed to preserve that distinction: view filters narrow the current
list, while server hard filters decide whether a job can prepare, queue, or
submit.

## Fix Summary

- Added a typed, validated URL contract for every match-view filter.
- Preserved preview/scenario parameters while removing stale route state.
- Restored filters after refresh, direct navigation, and browser back/forward.
- Limited match selectors and counts to active Career Tracks.
- Normalized remote, hybrid, on-site, in-person, and office workplace labels.
- Changed workplace filtering from substring matching to exact categories.
- Allowed a 100% minimum-fit selection.
- Added distinct empty states for filtered results, passed jobs, selected
  Career Tracks, outside-rule jobs, and accounts with no discovered jobs.
- Reset pagination whenever any filter changes.
- Added focused tests for invalid URLs, whitespace queries, score clamping,
  inactive tracks, combined filters, workplace variants, and reset behavior.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/lib/match-filters.ts` | Typed URL state, normalization, filtering, and reset helpers |
| `jobs/portal/src/lib/match-filters.test.ts` | Ten focused filter regression tests |
| `jobs/portal/src/views/MatchesView.tsx` | URL-backed controls, active-track scope, truthful empty states, and exact workplace matching |
| `jobs/portal/src/App.tsx` | Preview-only navigation state for links leaving Matches |
| `web/jobs/index.html` and `web/jobs/assets/*` | Rebuilt production portal bundle |

## Edge Cases Handled

- Whitespace-only and repeated-whitespace searches.
- Invalid, inactive, or deleted Career Track IDs in a saved URL.
- Negative, non-numeric, non-step, and greater-than-100 score parameters.
- Unknown workplace parameters and mixed workplace labels.
- Prepared jobs hidden while other filters remain active.
- Passed-job mode with no rows in the selected Career Track.
- Outside-rule review without changing server eligibility.
- Pagination under 125-row preview data.
- Mobile filter dialog, compact rows, light theme, and dark theme.
- Settings navigation without leaking Matches-only parameters.

## How to Test

```bash
npm test --prefix jobs
npm run typecheck --prefix jobs
npm run build --prefix jobs
cargo test --manifest-path server/Cargo.toml
cargo fmt --manifest-path server/Cargo.toml -- --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
node jobs/scripts/ci-guards-self-test.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
node scripts/check-bluey-edge-policy.mjs
git diff --check
```

Use `/jobs/matches?preview=1` for manual desktop/mobile checks. Combine `q`,
`track`, `score`, `workplace`, `unprepared`, `outside`, `passed`, and
`density`, then refresh and use browser back/forward.

## Known Limitations

- Match-view filters do not change Career Track policy. Location, employment
  type, engagement type, authorization, experience, sponsorship, company
  collision, freshness, ATS capability, and daily limits remain server-owned.
- This fix does not enable model generation, local Browser distribution, cloud
  Browser distribution, or universal unattended submission.
- R2 replication remains an independent production blocker while the current
  credential returns `AccessDenied`.
