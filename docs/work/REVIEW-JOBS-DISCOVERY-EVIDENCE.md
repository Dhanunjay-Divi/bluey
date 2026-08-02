# REVIEW: JOBS-DISCOVERY-EVIDENCE - Verified Discovery Execution Gates

> **Codex preflight:** Loaded `$bluey-ops` before review and verified its memory
> against the current repository state and commit scope.

**Commit range:** `bae308bb..working tree`
**Reviewer:** Codex self-review
**Date:** 2026-08-02

## Per-Task Review

### JOBS-DISCOVERY-EVIDENCE - Server-authoritative source truth

| Field | Value |
|-------|-------|
| Files | Automation classifier/tests; Jobs evidence, import, eligibility, persistence, API, fixtures, and tests |
| Verdict | ACCEPT |

**Findings:**

- Missing evidence defaults to unknown and fails closed.
- Evidence is bound to the server-derived canonical job key and URL host.
- Feed provenance is not promoted to original-source verification.
- Hosted ATS evidence is limited to Review-first packet preparation.
- Closed, mismatched, stale, duplicate, reposted, impersonated, and blocked
  postings cannot reach execution.
- Runner-plan fixtures derive their canonical key with production code, avoiding
  test-only evidence that could mask a binding error.

## Cross-Task Findings

- The same decision object reaches preparation, queueing, receipt, mailbox, and
  finalization paths. No second client-owned safety decision was introduced.
- The unrelated untracked Browser packaging scripts are intentionally outside
  this review and commit.

## Build & Test Verification

```bash
cargo fmt --all --check                         # passed
cargo clippy --all-targets -- -D warnings       # passed
cargo test --quiet                              # passed
npm run test --workspace @bluey/jobs-automation # 223 passed
npm run typecheck --workspace @bluey/jobs-automation # passed
npm run build --workspace @bluey/jobs-automation     # passed
```

## Overall Verdict

**ACCEPT** - Ready to commit as an isolated safety slice.

## Follow-ups for Next Batch

- Durable runner lease recovery and side-effect-unknown reconciliation.
- Source revalidation workers, canaries, and provider kill switches.
