# FIX-695: Substring Classification Could Manufacture Executable Job Evidence

> **Codex preflight:** Loaded `$bluey-ops` and reconciled role, experience, geography, workplace,
> category, eligibility, and execution rules against the current Phase 613 worktree. No external
> job source, account, application, message, or submission was used.

## Issue

Scattered substring and first-match logic could turn ambiguous roles, locations, workplace text,
or employment/engagement prose into positive hard-filter evidence and allow an unproven posting to
reach runner eligibility.

## Root Cause

Role-family inference combined Track text with posting text, so a configured target could
contaminate independent posting classification. Experience classification also inspected
employment highlights, allowing a mention of another discipline to reclassify the employment
title. Short aliases, overlapping role phrases, seniority layers, and multiple distinct title
spans did not share one deterministic contract.

Location and workplace checks used lowercase containment. They could broaden an exact city into a
metro, collapse country/subdivision tokens such as `CA` or `IN`, treat excluded jurisdictions as
positive scope, or treat `not remote` and mixed workplace text as remote. Employment and
engagement inference combined typed fields, titles, and descriptions and selected the first
matching signal. Unknown, mixed, or negated evidence could then avoid a failure because queue
admission tracked the absence of a known mismatch instead of positive proof for every category.

## Fix Summary

- Add one digest-bound server taxonomy with separate target-role resolution and posting-title
  classification. Known aliases resolve deterministically; `PM`, `TPM`, distinct role spans,
  unknown titles, and terse alias substrings remain ambiguous or unknown.
- Classify relevant experience from the employment title only and keep seniority assessment
  independent from role-family membership.
- Add typed country, subdivision, metro, exact-city, and workplace normalization. Conflicts,
  missing context, relational exclusions, negated workplace claims, mixed evidence, and unknown
  values return explicit review states instead of a positive match.
- Match skills through registered aliases with token and symbol boundaries so `Go`, `R`, `C`,
  `C#`, and `C++` remain distinct. Bare `Go`, `R`, and `C` free-prose tokens are not proof;
  contextual language aliases remain recognized.
- Make the portal's non-authoritative experience presentation mirror the server's longest-span
  role semantics, including layered seniority, distinct-family ambiguity, and safeguards against
  promoting terse `QA`/`DE` substrings.
- Classify employment and engagement from typed posting evidence without title inference. Multiple
  positive kinds or a negated kind become unknown; Track policy decisions surface explicit
  unverified review reasons.
- Require positive proof booleans for role, geography, employment, engagement, and both Track
  category checks before local or cloud queue authority can be true.
- Treat an invalid persisted sponsorship policy as a hard failure instead of falling through to a
  passing default.

## Files Modified

| File | Change |
|------|--------|
| `jobs/taxonomy/canonical-v1.json` | Freeze role, skill, country, subdivision, metro, and city authority |
| `server/src/jobs_taxonomy.rs` | Validate the registry and implement bounded role, skill, workplace, and geography classification |
| `server/src/db/jobs/candidate_policy.rs` | Use independent role evidence and typed, negation-aware employment/engagement decisions |
| `server/src/db/jobs/eligibility.rs` | Require positive hard-filter proof before queueing and fail closed on invalid policy values |
| `server/src/db/jobs/{profile_postings,operational_holds}.rs` | Apply canonical scoring, location decisions, and exact region hold scopes |
| `server/src/db/jobs/tests.rs` | Add positive controls and adversarial queue-authority regressions |
| `jobs/portal/src/lib/{canonical-taxonomy,search-policy,preview-application}.ts` | Mirror bounded taxonomy semantics for presentation without creating server authority |
| `jobs/portal/src/data/career-suggestions.ts` | Derive target-role suggestions from the frozen registry |

## Edge Cases Handled

- deterministic `SWE`, `SDE`, `CRA`, and `CRC` versus ambiguous `PM` and `TPM`;
- contained aliases in `Technical Product Manager` versus distinct spans such as
  `Product Manager / Project Manager`;
- terse aliases embedded in unrelated titles, layered seniority, and bounded level suffixes;
- `Entry Level Senior Software Engineer` and other layered portal seniority labels;
- highlight mentions that must not reclassify an employment title;
- `Go` versus `Django`, `R` versus `Rust`, `C`/`C#`/`C++` symbol boundaries, and prose such as
  `go to market`, `R&D`, and `C-suite` versus contextual language aliases;
- `CA`/`IN`, `New York`, unknown cities, cross-subdivision metros, and country conflicts;
- compact, connector, and suffix jurisdiction exclusions such as `non-US`, `but not California`,
  `anywhere but in California`, and `California not available`;
- `not remote`, `not currently remote`, `no longer remote`, temporal unavailability,
  `remote or hybrid`, and unsupported workplace text;
- `Contract Administrator` and `1099 Compliance Analyst` without typed category evidence; and
- unknown, negated, or mixed employment/engagement evidence with an exact positive queue control.

## How to Test

Observed during the focused implementation checkpoint:

```bash
cargo check --manifest-path server/Cargo.toml --lib
# PASS

cargo clippy --manifest-path server/Cargo.toml --lib --tests -- -D warnings
# PASS

cargo test --manifest-path server/Cargo.toml --lib jobs_taxonomy::tests
# PASS: 14 tests

cargo test --manifest-path server/Cargo.toml --lib candidate_policy_tests
# PASS: 5 tests

cargo test --manifest-path server/Cargo.toml --lib \
  typed_job_categories_gate_queue_authority_and_preserve_positive_controls
cargo test --manifest-path server/Cargo.toml --lib \
  unknown_contract_engagement_requires_review_instead_of_guessing
cargo test --manifest-path server/Cargo.toml --lib \
  negated_c2c_evidence_never_satisfies_c2c_only_track
cargo test --manifest-path server/Cargo.toml --lib \
  workplace_evidence_cannot_bypass_remote_only_queue_authority
# PASS: 4 focused eligibility regressions
```

Those focused results were reconfirmed by the clean all-target Rust command: 1,517 tests passed
with zero failures or ignored tests, after fmt, check, and Clippy passed. Schema parity passed at
81 tables/74 indexes, the whole-diff privacy gate passed over 2,622 tracked/2,347 text files, the
portal production build processed 2,299 modules with its existing size advisory only, and the local
disposable-PostgreSQL authority suite passed 13 tests. Exact-tip CI and hosted/provider evidence
remain separate gates.

## Known Limitations

- Unknown custom target roles remain visible and reviewable but cannot receive executable policy
  authority in this round.
- Taxonomy coverage is intentionally finite; expanding roles, skills, or geographies requires a
  new reviewed registry version and activation rather than runtime guessing.
- Original-source verification, real ATS canaries, and live provider behavior remain successor or
  external evidence; this fix does not enable discovery or submission.
