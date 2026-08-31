# FIX-756 — Composed signed integrity in Auto-submit eligibility

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 production-readiness and positive-route reachability blocker

**Status:** Implemented in source; the final certified sweep is green. The composed-preflight
static guard, configured-PostgreSQL, aggregate, and independent-review evidence remain pending.

## Issue

Bluey's workspace, approval, and effect boundaries already projected the current Phase 614 source,
exact ATS certification, and signed Phase 614B employer/risk authority as one composed decision.
Application preparation and its precommit finalization check did not. They projected the original
source, then read ATS certification independently, but never overlaid the signed employer identity
and job-risk result into the posting used by `build_job_eligibility`.

Production sanitization intentionally removes mutable `verified` employer and `clear` risk labels.
The Phase 614 source projection can restore only `ats_tenant_verified` and `source_screened`.
Therefore a legitimately signed positive job remained `employer_identity_review_required` and
`job_risk_review_required`, and public Auto-submit could never reach `queued` even though every
current authority was positive.

## Root Cause

FIX-730 made mutable eligibility representation fail closed and left a documented seam for the
Phase 614B integrity overlay. The later signed-integrity implementation connected that overlay to
workspace, approval, reservation, runner, and Final Submit boundaries, but the shared
application-preflight evaluator retained its source-only shape. Its separate ATS lookup also used a
different read/time from the integrity authority that had been signed for an exact ATS target.

The public production-positive fixture initially hid this defect by inheriting the older mutable
positive test shape. Once the fixture was moved completely onto public source, ATS, integrity,
profile, authorization, preparation, and finalization paths, the real review-only result became
visible.

## Fix Summary

- Public `evaluate_job_eligibility` resolves one `ComposedJobIntegrityProjection` and evaluates a
  clone whose `discovery_evidence` is replaced by that signed composition.
- `prepare_application_inner` and `finalize_prepared_application_kit` each resolve one composed
  source/ATS/integrity snapshot for their reversible preflight and pass it to the shared evaluator.
- The frozen posting fingerprint remains based on the Phase 614 source-projected posting. The
  signed overlay changes only the evaluator clone's discovery evidence; it does not silently change
  the job URL, source, or resume target fingerprint.
- ATS capability is applied only from `composed.ats_certification`. The former independent ATS
  reread was removed, so eligibility cannot combine a signed integrity result with a different ATS
  read or clock.
- The existing atomic prepared-application commit keeps its separate in-transaction composed
  authority recheck immediately before queue/approval persistence. The preflight result is not
  treated as effect authority.
- The certified fixture now proves a real score at or above the candidate threshold, no unresolved
  requirements, `can_auto_submit = true`, and the exact requested certified runner before final
  queueing.
- A static regression pins the composed overlay and forbids an independent ATS resolver in the
  prepare/finalize preflight path.
- The runner-volume certified claim regression was moved off direct queued-row/receipt fabrication
  and onto the same public signed application fixture with its exact cloud runtime target.

## Files Modified

| File | Change |
| ---- | ------ |
| `server/src/db/jobs/eligibility.rs` | Evaluate against one composed signed projection and exact composed ATS result. |
| `server/src/db/jobs/applications.rs` | Bind prepare/finalize preflights to one composition and add the static guard. |
| `server/src/db/jobs/tests.rs` | Prove public score, exact runner eligibility, queueing, and expose the shared test fixture to sibling authority tests. |
| `server/src/db/jobs/runner_volume_purge.rs` | Replace the certified runtime claim's fabricated queued application with the public signed fixture. |
| This FIX and Phase 614B records | Record the production reachability defect and final evidence. |

## Security And Concurrency Properties

- Mutable posting JSON still cannot mint employer identity, clear risk, ATS capability, or queue
  authority.
- SQLite composition uses one deferred transaction and one database timestamp.
- PostgreSQL composition preserves `H -> M -> ATS -> D -> integrity publication fence` and samples
  one database timestamp after those locks.
- Preparation and generation remain reversible. The irreversible boundary independently reacquires
  the complete authority snapshot and fails closed on revocation, expiry, source/ATS drift, Track
  change, entitlement change, or authorization change.
- A missing, expired, revoked, blocked, or mismatched integrity authority cannot become positive by
  falling back to a second ATS lookup.
- The signed overlay is account-independent job authority only. Candidate/profile/Track/entitlement
  facts remain in their existing account-bound authorities.

## Evidence

- **PASS — final certified sweep:** `certified_`, 15 passed and 0 failed in 41.82 seconds. This
  supersedes the diagnostic 14/15 run and includes the repaired runner-volume fixture alongside
  the signed local/cloud paths.
- **PENDING — final frozen-snapshot focused rerun:** composed-preflight static guard and shared
  Node/Rust vector.
- **PENDING — final frozen-snapshot configured PostgreSQL, aggregate Rust, strict Clippy, Jobs
  JS/TS/schema/privacy/release gates, and independent line-by-line review.**

Diagnostic checkpoints are not final-source release evidence.

## How To Test

```bash
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  preparation_eligibility_uses_one_composed_authority_without_an_ats_reread -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  certified_cloud_runtime_fixture_authorizes_exact_execution_lease_claim -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  certified_cloud_production_path_is_single_submit_even_after_response_loss -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  certified_local_production_path_is_single_submit_even_after_response_loss -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib certified_ \
  -- --nocapture --test-threads=1

# Use only the isolated disposable PostgreSQL 17 authority database.
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib postgres_ -- --nocapture --test-threads=1
```

## Known Limitations

- This fix makes the signed positive Auto-submit path reachable and keeps it fail closed; it does
  not authorize a deployment, enable a flag, contact a provider, submit an application, or prove a
  hosted runner/hosted-PostgreSQL environment.
- The certified DB-level Final Submit fixture is not a substitute for full API
  `commit_packet`/workflow-start orchestration evidence.
- Hosted PostgreSQL interruption/failover, production signing-key custody, runtime image
  attestation, canaries, rollback, cohorts, and live flag read-back remain external release gates.
