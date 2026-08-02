# IMPL: JOBS-DISCOVERY-EVIDENCE - Verified Discovery Execution Gates

> **Codex preflight:** Loaded `$bluey-ops` before implementation and verified
> its memory against the current repository state.

## Scope

**Does:**

- Defines explicit discovery provenance, canonical-job, employer, scam-risk,
  and original-source evidence on every job posting.
- Fails closed when evidence is missing, stale, mismatched, duplicated,
  reposted, malformed, closed, impersonated, or blocked by risk screening.
- Binds discovery evidence to the exact canonical job key and application
  destination domain.
- Permits an allowlisted hosted-ATS snapshot to produce a Review-first packet
  without treating that snapshot as independent employer or scam verification.
- Requires fresh, fully verified original-source evidence before a posting may
  enter an application runner queue.
- Reuses the same eligibility decision for preparation, queueing, finalization,
  receipts, mailbox processing, and runner-plan tests.

**Does NOT:**

- Enable model generation, local Browser distribution, cloud Browser
  distribution, mailbox sync, or unattended employer-facing execution.
- Add a scheduled original-source revalidation worker or new public job feeds.
- Treat external curated feeds as application truth.
- Stage the unrelated untracked Browser packaging scripts present in this
  checkout.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/automation/src/discovery-quality.ts` | Created | Shared TypeScript discovery-quality classifier for ranking and execution boundaries |
| `jobs/automation/tests/discovery-quality.test.ts` | Created | Covers external leads, hosted ATS review, verified queueing, and fail-closed states |
| `server/src/db/jobs.rs` | Modified | Adds `JobDiscoveryEvidence` and immutable scam signals to postings |
| `server/src/db/jobs/eligibility.rs` | Modified | Applies canonical, employer, domain, risk, freshness, and source gates |
| `server/src/api/jobs_import.rs` | Modified | Assigns truthful evidence to imported jobs without overstating verification |
| `server/src/db/jobs/discovery.rs` | Modified | Persists discovery evidence with canonical jobs |
| `server/src/db/jobs/global_materialization.rs` | Modified | Materializes external-feed leads as unverified execution inputs |
| `server/src/api/jobs.rs` | Modified | Returns and consumes the server-owned evidence contract |
| Jobs fixtures and tests | Modified | Bind evidence to real fixture canonical keys and exercise all execution stages |

## Trust Levels

1. **External feed lead:** rankable, but cannot prepare or queue until the
   original employer source is revalidated.
2. **Hosted ATS source snapshot:** can prepare a Review-first application kit
   when its canonical key and destination domain match. It cannot queue because
   provider presence alone is not independent employer/scam verification.
3. **Verified original source:** may queue only when the employer identity,
   canonical job, application domain, scam-clear result, evidence hash, and
   freshness window all match and every other Career Track eligibility rule
   passes.

## Build & Test

```bash
npm run test --workspace @bluey/jobs-automation -- --reporter=dot
# 28 files, 223 tests passed

npm run typecheck --workspace @bluey/jobs-automation
npm run build --workspace @bluey/jobs-automation
# success

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --quiet
# 818 Rust unit tests, 77 integration tests, runner matrix, and support suites passed

cargo test --test jobs_runner_plan_matrix
# passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Hosted ATS evidence remains Review-first | An ATS tenant snapshot proves a source response, not independent employer identity or scam clearance |

## Known Follow-ups

- Deploy the scheduled original-source revalidation and risk workers before
  enabling broad discovery execution.
- Add operational canaries and alerts for source drift, stale evidence, and
  employer/domain mismatches.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
