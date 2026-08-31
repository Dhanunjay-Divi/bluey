# FIX-784: Runner plan matrix stopped at public-beta admission

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this correction against the current
> Phase 621/622 aggregate worktree. No production route, middleware, authorization rule, release
> flag, provider, credential, cohort, or deployment state was changed.

## Issue

The exact PR #35 Jobs run at aggregate commit
`60fb5f3e0e9b27c034d7443c706c5a4cc7f28093` failed the server integration test
`free_pro_cloud_entitlements_remain_observable_but_review_first_blocks_effects`. The first local
runner assertion at line 685 expected HTTP 409 but received HTTP 403 in
[run 33361005550, job 99392200301](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33361005550/job/99392200301).

## Root Cause

`server/tests/jobs_runner_plan_matrix.rs` predated the Phase 621 public-beta middleware. Its
`TestContext::boot` left the durable cohort at the migration default `draft`, while
`TestContext::account` created an unverified account without a durable public-beta enrollment.
The production middleware therefore correctly rejected the request before
`queue_application_run` could reach the application-level integrity and plan boundaries the test
was intended to exercise. Merely changing the expected response from 409 to 403 would have
preserved a passing test while abandoning its declared regression coverage.

## Fix Summary

The isolated integration fixture now:

- creates a real verified administrator and opens a one-hour, 16-account cohort through the
  audited production mutation boundary;
- verifies every matrix account and enrolls it through the production public-window admission
  function;
- reads back the durable `public_window` enrollment row before assigning plan entitlement; and
- asserts the existing local/cloud signed job-integrity HTTP 409 after that admission proof while
  retaining the metering, application, session, reservation, and workflow-command assertions.

The test name and comments now describe the boundary actually proved: beta-admitted Free/Pro/Cloud
entitlements remain observable while application effects fail closed without mutation. They do
not claim that this fixture reaches the later queue-state-specific review message, because the
independent signed-integrity guard intentionally evaluates first.

Production middleware and runtime authority remain unchanged. The fixture now proves it has
crossed public-beta admission before it evaluates the intended application-integrity and plan
behavior.

## Files Modified

| File | Change |
|------|--------|
| `server/tests/jobs_runner_plan_matrix.rs` | Establish and read back bounded durable beta admission before exercising the existing plan/integrity boundaries |
| `docs/work/FIX-784-public-beta-runner-plan-matrix-fixture.md` | Record the hosted failure, root cause, and bounded correction |
| `CHANGELOG.md` | Record the test-fixture correction under Unreleased |

## Edge Cases Handled

- Cohort activation uses a current verified administrator and the audited compare-and-swap path;
  it does not bypass the production authority with direct cohort SQL.
- Every candidate account is verified and admitted independently; the test cannot accidentally
  pass because another account consumed or shared admission.
- The hard cap and half-open window are bounded but comfortably cover the seven fixture accounts.
- Durable enrollment read-back proves HTTP requests crossed the outer beta boundary without
  fabricating the independent signed job-integrity authority that this matrix deliberately lacks.
- Because signed job integrity is checked before queue state in production, the later
  `awaiting_review` message is intentionally unreachable in this fixture. The exact
  `original_source_verification_inactive` assertion proves the preexisting application-level
  fence instead of weakening or bypassing it.
- Entitlement metering, application state, Browser sessions, reservations, and workflow commands
  retain their existing no-mutation assertions.

## How to Test

```bash
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo +1.95.0 test --manifest-path server/Cargo.toml \
  --no-default-features --features integration-test-support \
  --test jobs_runner_plan_matrix \
  free_pro_cloud_entitlements_remain_observable_while_effects_fail_closed \
  -- --exact --nocapture

cargo +1.95.0 fmt --manifest-path server/Cargo.toml --all -- --check
cargo +1.95.0 clippy --manifest-path server/Cargo.toml \
  --no-default-features --features integration-test-support \
  --test jobs_runner_plan_matrix -- -D warnings
scripts/check-bluey-ops-docs.sh
git diff --check
```

## Known Limitations

- This focused correction does not replace the required fresh exact-tip hosted Jobs run and full
  server verification before release.
