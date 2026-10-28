# FIX-812: Prospective migrations escaped the unstaged whitespace check

> **Codex preflight:** `bluey-ops`; current source/index and observed output are authoritative.

## Issue

The local checkpoint stopped before commit: staged diff checking found one extra
blank line at EOF in each new SQLite066/PostgreSQL044 migration. Earlier ordinary
git diff checks did not include these then-untracked files. No commit or external
write was made by the failed checkpoint attempt.

## Root Cause

Untracked prospective source is absent from ordinary git diff. Privacy used a
temporary complete index but had not also run the whitespace check there.

## Fix Summary

Remove exactly one trailing newline from each new migration, retaining one final
newline. Compare each file to its pre-format staged bytes and require exact
equality after only trailing-newline normalization. No SQL token, historical
migration or runtime authority changes. Require a complete prospective-index
whitespace check before future heavy source freezes, not only before commit.

## Files Modified

| File | Change |
|------|--------|
| `infra/sqlite/server-runtime/066_jobs_attempt_employer_identity.sql` | One EOF-only blank-line removal. |
| `infra/postgres/server-runtime/044_jobs_attempt_employer_identity.sql` | One EOF-only blank-line removal. |

## Edge Cases Handled

- Existing final newline is retained; normalization cannot hide another byte edit.
- Inherited untracked REVIEW628 remains outside the prospective candidate.
- Newly added source participates in checks instead of appearing only at staging.

## How to Test

Require exact before/after comparison and one-byte length difference per file;
run schema parity and CI-guard mutation self-tests, then prospective indexed
whitespace/privacy checks. The next full final-source gate must include the
formatted bytes together with pending warning fixes.

## Known Limitations

Rust88910/Jobs55179 full gates qualified the pre-format bytes and remain valid
dated snapshots, not exact current-byte evidence. The SQL normalized-byte proof
and fresh parity pass establish this limited formatting delta; no new full
server/Jobs execution is claimed after it. Exact final-source requalification
remains required before release alongside the P2 font/P3 bundle-quality holds.
