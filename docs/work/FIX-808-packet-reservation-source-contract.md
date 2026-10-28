# FIX-808: Packet reservation source contract retained its obsolete row bound

> **Codex preflight:** `bluey-ops`; current Phase636 source and local evidence are authoritative.

## Issue

Session16096's full library run failed
`packet_commit_recomposes_current_effect_eligibility_in_both_dialects` at
applications.rs:2871 with `SQLite omits bounded LIMIT 1`.

## Root Cause

The old source-contract test required one matching company row. Canonical employer
admission now deliberately reads the owned application first plus one blocking peer
(`LIMIT 2`), with a separately bounded daily-budget query. The assertion lagged the
reviewed product change; changing product SQL back would lose owned-binding validation.

## Fix Summary

Replace the obsolete assertion with explicit checks in both SQL dialects for the
11-column binding projection, own-row inclusion/ordering, name/ID/domain peer matching,
legacy-null holds, active-state predicates and the two-row bound. Retain bounded
daily history, clamped daily limit, PostgreSQL shared locks and negative unbounded
account-history checks. Normalize whitespace only for source assertions.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/applications.rs` | Test-only current reservation-query contract. |

## Edge Cases Handled

- Owned binding cannot be displaced by a peer before validation.
- Unknown legacy employer binding remains a blocking input.
- Daily-budget and employer-history queries keep independent bounds.

## How to Test

Run the exact named library test through the canonical isolated launcher before
the private PostgreSQL and full all-target gates. The successor qualification
controller records the exact command and source manifest.

## Known Limitations

Successor88910 passed the exact corrected source-contract case1/1 (0.00s) before
the other fixture/quota preflights. Full current-source gates remain pending. A
source-contract test does not replace actual SQLite/PostgreSQL transaction and
concurrency tests. Session16096
finished with1726 passed/6 failed; remaining targets did not run. All25 frozen
hashes and HEAD matched afterward, the owned root was absent, its parent empty
and observed owned processes absent.
