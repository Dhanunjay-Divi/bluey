# FIX-724: Certified Fixtures And Source Tests Overstated Execution Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against Phase 614 source-evidence
> scope, the Phase 614B employer/risk boundary, and the pre-final Rust library triage. The SSD
> archive was not used.

**Status:** Implemented; focused regressions green, aggregate remains non-green

## Issue

Two certified ATS fixtures used the execution-grade, test-only original-source label even though
they represented provider verification only. Two older source/category regressions also expected
provider evidence and positive category controls to mint queue authority, contrary to the current
Review-first boundary.

## Root Cause

Historical positive fixtures combined provider provenance with independent employer/risk
authority. Phase 614 separated those concepts: provider evidence may support preparation, but it
cannot mint the signed employer/risk authority owned by Phase 614B. The stale labels caused
`ScopeMismatch` before the tests could reach that intended downstream denial, while the stale
expectations treated the denial itself as a regression.

## Fix Summary

- Replace the two certified-fixture source labels with the honest
  `provider_verified_original_source` scope.
- Preserve the Phase 614B employer-identity/current-authority denial instead of adding a test-only
  employer/risk bypass.
- Update the historical source/category regressions to assert Review-first preparation and no local
  or cloud queue authority.
- Keep the non-green subset outcomes visible rather than counting expected fail-closed denials as
  passes.
- Clean up the execution-lease/local-run intervention fixtures without using the public queue gate
  to fabricate missing Phase 614B authority: bind provider-verified source evidence, retain the
  valid approved-execution envelope/checksum, and persist a test-only preapproved queued row.

## Files Modified

| File                                                                          | Change                                                                                          |
| ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `server/src/db/jobs/tests.rs`                                                 | Repin certified fixture scope and two source/category expectations to the Review-first boundary |
| `docs/work/FIX-724-jobs-certified-fixture-scope-review-first-expectations.md` | Record the fixture correction, focused evidence, and remaining authority gap                    |
| Phase 614 Round/IMPL/REVIEW/CHANGELOG                                         | Include the correction without making an aggregate-green claim                                  |

## Edge Cases Handled

- Provider-verified source evidence can support preparation without authorizing queue or effect.
- Typed category positive controls do not substitute for employer/risk authority.
- Certified ATS fixtures reach the intended authority boundary without a source-scope mismatch.
- The eight intentional Phase 614B denials remain failures in the bounded subset report.
- The local intervention fixture seeds the reservation prerequisite; the cloud case checks that a
  successful claim would advance the reservation to `running` and uses the real fixture checksum
  for invalidation.

## How To Test

```text
current_provider_source_evidence_allows_preparation_but_not_queueing                   1 / 1 (0.07s final source)
typed_job_categories_preserve_positive_controls_without_minting_queue_authority       1 / 1 (2.84s final source)
Certified fixture triage subset                                                       6 passed / 8 expected authority denials (earlier checkpoint)
ScopeMismatch in certified subset                                                     0
Latest focused cloud/local intervention diagnostics                                   0 / 2
  cloud                                                                               Conflict at claim_execution_lease
  local                                                                               reaches running update; match score below Auto-submit threshold
Rust fmt and scoped diff check                                                        passed
Server cargo check --all-targets                                                      passed (34.06s)
Server strict Clippy, -D warnings                                                     passed (49.41s)
Full Rust library/all-target aggregate                                                not green; rerun pending
```

The earlier certified-subset checkpoint used `server/src/db/jobs/tests.rs` SHA-256
`142aa829a337d4e34a1194a64b203dbc8829becb7495ebc161338e18ab1be320`.
Later honest-fixture cleanup produced final current source SHA-256
`0f4775b375c755c63c6563b3885788681f7a45678102058f631dd68935bec325`; the two named Review-first
regressions reran green on those exact final bytes, while the earlier 6/8 subset count remains
checkpoint-specific.

The 0/2 intervention result is diagnostic evidence only. Cloud still fails at
`claim_execution_lease` with `Conflict` in the current entitlement/shared execution-authority path.
Local advances to `update_application(..., "running")` and then fails `match score below Auto-submit
threshold`. A threshold tweak was removed because `save_profile` canonicalizes it to 80. These
results expose the deeper shared positive-authority/fixture blocker; they are not Phase 614
acceptance evidence.

## Known Limitations

- The subset proves fixture-scope repair and the intended fail-closed boundary; it is not a green
  aggregate result.
- The cleaned intervention fixtures remain 0/2 and do not retire either baseline failure.
- A production-representative positive queue route still requires confirmed sponsorship and the
  separately reviewed Phase 614B signed employer/risk authority.
- Full library tests and live external gates remain pending; final-source fmt/check/strict Clippy
  and scoped diff are green.
