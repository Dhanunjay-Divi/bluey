# REVIEW: JOBS-COMMUNICATION-ACTIONS - Reviewed Communication Authority

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state and commit scope.

**Commit range:** `176f048c..working tree`
**Reviewer:** Codex self-review
**Date:** 2026-08-02

## Per-Task Review

### JOBS-COMMUNICATION-ACTIONS - Encrypted Approval Queue

| Field | Value |
|-------|-------|
| Files | Jobs communication DB/API, two runtime schemas, tests, docs, changelog |
| Verdict | Green: accept |

**Findings:**

- Payload size, recipients, attendees, subject/body length, and calendar duration are bounded.
- Replies require a real same-account inbound provider message bound to the application.
- Public summaries do not disclose action content or private worker state.
- Approval is separate from creation, and cancellation prevents dispatch.
- Expired possible-side-effect operations transition to `side_effect_unknown` rather than retrying.
- Provider success requires a real provider object ID under a matching lease token and fence.

## Cross-Task Findings

- Provider execution must remain a separate private-worker slice. Publishing claim or
  completion endpoints on the account-facing router would weaken this authority boundary.
- Mailbox synchronization must stay disabled until OAuth revocation, provider evidence,
  and ambiguous-side-effect reconciliation pass authorized sandbox tests.

## Build & Test Verification

```bash
(cd server && cargo test communication_ --quiet)  # 7 passed
cargo fmt --all --check                            # passed
node jobs/scripts/check-jobs-schema-parity.mjs     # passed
node jobs/scripts/ci-guards-self-test.mjs           # passed
git diff --check                                    # passed
```

## Overall Verdict

Green: **ACCEPT** - Ready to commit as a bounded authority slice. This verdict does
not certify provider sending or enable mailbox synchronization.

## Follow-ups for Next Batch

- Build provider workers and private authentication.
- Add Gmail and Microsoft sandbox fixtures and live authorized certification.
- Add portal review and intervention UX.
