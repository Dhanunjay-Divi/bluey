# FIX-696: Public ATS Normalization Inferred Categories and Dropped Valid Candidates

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the public ATS acquisition boundary in the
> current Phase 613 worktree. No live ATS request, authenticated provider session, employer
> contact, application, or submission was used.

## Issue

The public ATS worker could manufacture typed employment, engagement, or workplace evidence from
prose and could permanently discard candidates through raw positive role/location substring
filters before the server taxonomy evaluated them.

## Root Cause

Normalization concatenated provider categories with the title and description, then returned the
first category substring. `Contract Administrator`, benefits prose, or a description mentioning
W2/C2C could therefore become hard-filter evidence. Several adapters selected one typed field with
`a || b`, hiding contradictions such as Workday `Full time` plus `Temporary`.

Workplace inference used unbounded `includes("remote")`; SmartRecruiters `remote: false` was not
preserved, and a city literally named `Remote` could become a remote job. Negated and mixed values
also selected a positive kind. Finally, `matchesQuery` enforced desired roles and locations with
raw substrings, so aliases and typed geography that only the server understood never reached the
authoritative classifier. Workday additionally received the raw role string as `searchText`, which
could exclude alias-only candidates at the provider before Bluey saw them.

## Fix Summary

- Derive employment only from provider-owned typed employment fields, and derive engagement only
  from typed engagement/employment fields. Titles and descriptions remain review evidence, never
  category authority.
- Combine all relevant typed fields for Greenhouse, Lever, Ashby, SmartRecruiters, and Workday so
  contradictory provider values become unknown instead of silently preferring the first field.
- Require exactly one non-negated category signal. Negated, relationally rejected, or mixed
  employment/engagement values remain undefined for server review.
- Preserve SmartRecruiters `remote: true` and `remote: false` alongside `workplaceType`; a
  contradiction becomes unknown.
- Make only the provider's explicit workplace category eligible for worker normalization. Raw
  location text such as `Remote, OR` stays raw for server review, and mixed, qualified, temporal,
  coordinated, or `no longer` negation cannot become one positive kind.
- Stop applying positive raw role, location, or remote-only filters in the worker. Keep explicit
  company and title exclusions; the server taxonomy owns every positive role/geography decision.
- Send an empty Workday `searchText` rather than an unreviewed target-role string, preserving the
  pinned endpoint, bounded pagination, and complete-snapshot checks.
- When an ordinary SmartRecruiters or Workday search reaches the page cap, retain its bounded
  partial jobs only with an explicit warning. Scheduled closure snapshots reject the same cap so
  incomplete evidence cannot close an unseen posting.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/src/public-ats.ts` | Enforce typed, contradiction-aware categories and preserve server-owned positive classification |
| `jobs/automation/tests/public-ats.test.ts` | Add provider, negation, mixed-evidence, false-remote, and prefilter regressions |

## Edge Cases Handled

- `Contract Administrator` and `1099 Compliance Analyst` without typed category fields;
- `No C2C; W2 only`, `C2C or W2`, suffix/prefix rejection, and ineligibility wording;
- `No full-time; contract only`, `Full-time or contract`, and unavailable/disallowed categories;
- Workday `Full time` plus `Temporary` and equivalent duplicated provider fields;
- SmartRecruiters city `Remote`, explicit `remote: false`, boolean/workplace contradictions, and a
  location-only `Remote, OR` without affirmative typed workplace evidence;
- negated, mixed, coordinated, temporal, or `no longer` remote/hybrid/on-site evidence under a
  remote-only acquisition hint;
- a posting titled `SDE II` that does not contain the raw desired `software engineer` text; and
- a valid Austin posting requested with a raw New York hint that must still reach server
  classification; and
- a Workday alias-only posting that would be lost if the provider applied Bluey's raw target text;
  and
- capped SmartRecruiters/Workday ordinary search warnings versus fail-closed complete snapshots.

## How to Test

Observed on the current focused automation sources:

```bash
(cd jobs && npm run test --workspace @bluey/jobs-automation -- tests/public-ats.test.ts)
# PASS: 39 tests in the current combined public-ATS file

(cd jobs && npm run typecheck --workspace @bluey/jobs-automation)
# PASS

(cd jobs && npm run test --workspace @bluey/jobs-automation)
# PASS: 680 tests; 1 conditional test skipped across 37 passing files and 1 skipped file
```

The current public-ATS file count also includes the later bounded-continuation coverage recorded in
FIX-710. Across all five Jobs workspaces, Vitest passed 1,847 tests with that one skip across 144
passing files and one skipped file. The skipped
`playwright-submit-guard.integration.test.ts` Greenhouse multipart POST/delayed-beacon case had no
configured Playwright Chromium executable, and no fallback browser was used. Whole-diff privacy,
provenance, CI guards, release tests, and the workflows gate also passed. Exact-tip CI and real
provider canaries remain pending.

## Known Limitations

- Provider category fields are still untrusted evidence and require the server's canonical policy
  and original-source checks before execution.
- Positive role/location/workplace query values are acquisition hints at this layer, so this fix
  can fetch more candidates; bounded explicitly partial pagination, age limits, exclusions,
  deduplication, and server admission remain the controlling limits.
- This fix does not add a provider, bypass robots/terms, authenticate to an ATS, or enable any
  discovery or submission flag.
