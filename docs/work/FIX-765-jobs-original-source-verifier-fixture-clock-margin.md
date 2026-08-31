# FIX-765: Jobs original-source verifier fixture clock margin

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B
> and the current authoritative worktree. The SSD archive was not used.

**Severity:** P1 test-authority and release-gate reliability defect

**Status:** Implemented and locally verified. Focused consumers, the full 1,586-test library
aggregate, and the frozen-source PostgreSQL 17.10 `r8` manifest are green.

## Issue

The first exact-tip full Rust run completed 1,585 tests successfully but failed one test:
`db::jobs::original_source_verification_tests::public_projection_keeps_non_authoritative_jobs_visible_without_weakening_effect_gates`.
Fixture setup panicked in `managed_cloud_release_authority.rs` with
`issue production-positive verifier grant: InvalidRequest` before the public-projection assertions
ran.

The same test passed when run alone on the unchanged source. That exact-alone pass was diagnostic
evidence of schedule sensitivity, not sufficient aggregate release evidence.

## Root Cause

`original_source_verifier_runtime_long_horizon_test_fixture` requested both a 15-minute managed
activation lifetime and a 15-minute runtime-grant TTL. The fixture captured SQLite database time
before publishing the signed v2 trust policy, release, cohort, and activation, then grant issuance
read database time again after that setup.

SQLite's fixture clock is rounded to whole seconds. If signed-authority setup completed within the
same clock second, the requested grant expiry exactly matched the activation expiry and issuance
succeeded. If setup crossed a clock-second boundary under aggregate-suite load, the fresh grant
clock plus 15 minutes exceeded the activation expiry derived from the earlier clock. The production
grant boundary correctly rejected that request as `InvalidRequest`.

The UUID-backed SQLite fixture is isolated per test. The failure was therefore not caused by the
public projection, test ordering against a shared database, or process-global managed authority;
it was a deterministic missing setup margin exposed by suite scheduling.

## Fix Summary

- The long-horizon test activation lifetime is now 20 minutes.
- The requested runtime-grant TTL remains exactly 15 minutes.
- Named test constants declare the authority lifetime, grant TTL, and a minimum 60-second setup
  margin. The fixture asserts that its configured lifetime difference satisfies that margin; the
  actual configured difference is five minutes.
- Grant setup now asserts the persisted authority retained the requested bounded TTL exactly:
  `expires_at_ms - created_at_ms == grant_ttl_ms`.
- The production grant issuer still requires the complete requested TTL to fit within the active
  activation. It was not weakened, capped, or changed to hide fixture delay.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Give the long-horizon test authority explicit setup headroom and assert the actual persisted grant TTL. |
| `docs/work/FIX-765-jobs-original-source-verifier-fixture-clock-margin.md` | Record the failure, root cause, bounded test-only correction, and evidence posture. |

## Edge Cases Handled

- Fixture setup may cross one or more whole-second SQLite clock ticks without shortening the
  requested grant.
- Fast exact-alone execution and slower aggregate execution now exercise the same valid authority
  relationship.
- All three current long-horizon fixture consumers receive the same explicit timing margin.
- Future edits that reduce the declared lifetime difference below 60 seconds fail immediately in
  fixture setup.
- Grant expiry remains bounded by activation expiry through the unchanged production check.

## Evidence

- **PASS — diagnostic exact-alone reproduction on pre-fix source:**
  `public_projection_keeps_non_authoritative_jobs_visible_without_weakening_effect_gates` passed
  alone, confirming the full-suite failure was timing-sensitive.
- **PASS — post-fix focused:**
  `public_projection_keeps_non_authoritative_jobs_visible_without_weakening_effect_gates`.
- **PASS — post-fix focused:**
  `assignment_authority_expiry_and_revocation_are_typed_and_fail_closed`.
- **PASS — post-fix focused:**
  `public_sqlite_lease_defers_held_prefix_with_bounded_fair_recovery`.
- **PASS — final feature-off library aggregate:**
  `CARGO_INCREMENTAL=0 cargo test --locked --manifest-path server/Cargo.toml --lib` passed
  **1,586/1,586**, zero failed/ignored/measured/filtered, in `2178.55s` test time. Binary
  `server/target/debug/deps/bluey_server-0ad0add04d272872` has SHA-256
  `1eb9fb796a5789c53dfbacd7d47fc90f4347ddbbc15c50b99d61d6bd6e947f0b`.
- **PASS — final configured PostgreSQL authority manifest:** a database created immediately before
  the run, `bluey_phase614b_pg17_r8`, passed all 19 exact tests on PostgreSQL 17.10. Every exact
  invocation reported 1 passed / 0 failed / 1,585 filtered; summed observed real time was 30.49
  seconds. The manifest used the same frozen library binary and digest above.

## How to Test

```bash
# Original failing public-projection path.
CARGO_INCREMENTAL=1 cargo test --manifest-path server/Cargo.toml --lib \
  public_projection_keeps_non_authoritative_jobs_visible_without_weakening_effect_gates \
  -- --nocapture

# The other long-horizon fixture consumers.
CARGO_INCREMENTAL=1 cargo test --manifest-path server/Cargo.toml --lib \
  assignment_authority_expiry_and_revocation_are_typed_and_fail_closed -- --nocapture
CARGO_INCREMENTAL=1 cargo test --manifest-path server/Cargo.toml --lib \
  public_sqlite_lease_defers_held_prefix_with_bounded_fair_recovery -- --nocapture

# Aggregate Rust evidence after the focused checks.
CARGO_INCREMENTAL=1 cargo test --manifest-path server/Cargo.toml
```

Run the separately defined exact PostgreSQL 17 manifest against the isolated Phase 614B evidence
database after the final source is frozen. Do not use a shared or production database.

## Known Limitations

- The completed local aggregate and PostgreSQL evidence do not substitute for hosted PostgreSQL,
  Docker/Linux, CI, or provider/runtime evidence.
- This test-only correction does not provide hosted PostgreSQL, Docker/Linux, registry,
  production-key, canary, provider-write, deployment, or feature-activation authority.
