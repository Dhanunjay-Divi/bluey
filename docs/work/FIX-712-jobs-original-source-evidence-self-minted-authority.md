# FIX-712: Positive Original-Source Evidence Could Be Self-Minted

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the current Phase 614
> source and Round 614. The SSD archive was not used.

**Status:** Implemented; focused and final-parser Jobs checkpoints green, post-lifecycle final
aggregate pending

## Issue

Production-visible `JobDiscoveryEvidence` helpers could construct a posting that appeared to carry
canonical source provenance, verified employer, clear scam risk, `verified_open`, and freshness
without naming or validating an immutable verifier receipt/head.

The mutable projection remains useful for compatibility and Review-first preparation, but it
cannot be the root of execution authority under the Round 612/614 receipt contract.

## Root Cause

`JobDiscoveryEvidence::verified_original_source` previously produced positive-looking mutable
fields without a verification ID, receipt digest, head generation, assignment fence, verifier
release/runtime identity, or server-validated risk binding. Eligibility could validate those fields
and freshness but could not prove their immutable origin.

## Fix

- Production positive projection now comes from the current immutable receipt and monotonic head,
  with exact assignment, job, employer, provider/source target, URL/destination, expiry, receipt
  digest, head generation, release/runtime, and execution-capability agreement.
- A positive projection is consumable only while its exact assignment remains current and `idle`.
  Changed-byte replay quarantines the assignment, which makes any earlier positive head
  non-consumable without rewriting its immutable history.
- The positive convenience constructor is compiled only under `cfg(test)` and stamps an explicit
  test-fixture provenance marker. Production callers cannot invoke it.
- Mutable/imported discovery evidence is sanitized; positive-looking caller fields cannot become a
  verifier receipt or bypass current-head validation.
- Provider-hosted ATS evidence keeps only acquisition/classification value and remains
  `ats_tenant_verified`/`source_screened` Review-first input.
- Current source authority is rechecked transactionally at execution-capable preparation,
  `queued`/`running` persistence, lease/local-run boundaries, and final effect authorization.
- The worker supplies bounded observations. The server validates the observation/result tuple and
  derives receipt digest, status, risk/capability decisions, clocks, and head transition.

## Files

- `server/src/db/jobs.rs`
- `server/src/db/jobs/original_source_verification.rs`
- `server/src/db/jobs/eligibility.rs`
- `server/src/db/jobs/applications.rs`
- `server/src/db/jobs/discovery.rs`
- `server/src/db/jobs/execution_authority.rs`
- `server/src/db/jobs/execution_leases.rs`
- `server/src/db/jobs/local_runner.rs`
- `server/src/db/jobs/tests.rs`
- `server/tests/integration_e2e.rs`
- `server/tests/jobs_runner_plan_matrix.rs`
- paired Phase 614 migrations and focused tests

## Verification

Observed focused evidence:

```text
Rust original-source authority                 25 / 25; normal-parallel twice
Projection/effect regressions                   2 / 2
Execution-lease regressions                    13 / 13
Local-run regressions                           4 / 4
Runner-plan Review-first matrix                 2 / 2
Schema parity                                  95 tables / 79 indexes per dialect
Rust all-target check and strict Clippy         passed
Jobs final-parser aggregate                 1,884 passed / 1 skipped
Integration E2E                                86 passed / 22 failed / 108 total
```

All 22 integration failures are the same fail-closed shared approval fixture: HTTP `409`, `Confirm
the sponsorship answer before Auto-submit.` This is not a green integration result and requires a
production-representative confirmed-sponsorship fixture plus **Phase 614B — Signed Job Integrity
Authority**.
The focused lifecycle module is green after the final typed-authority correction. Full Rust
all-target tests and final aggregate guards remain pending. Live PostgreSQL concurrency is
unproven without an authorized test URL.

## Known Limitations

- Phase 614 does not create independent employer-identity or scam-risk clearance. Even a current
  source receipt remains Review-first unless **Phase 614B — Signed Job Integrity Authority** also
  exists.
- This fix does not implement source enrollment, scheduling, SLOs, live canaries, deployment, or
  production activation; those remain Round 615/external gates.
- It authorizes no authenticated provider action, provider write, application, or message.
- All production and provider-write flags remain `0`; no production read-back is claimed.
