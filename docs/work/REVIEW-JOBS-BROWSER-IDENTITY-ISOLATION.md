# REVIEW: JOBS-BROWSER-IDENTITY-ISOLATION - Browser Profile Authority

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state and commit scope.

**Commit range:** `0487b692..working tree`
**Reviewer:** Codex self-review
**Date:** 2026-08-02

## Per-Task Review

### JOBS-BROWSER-IDENTITY-ISOLATION - Fail-Closed Profile Binding

| Field | Value |
|-------|-------|
| Files | Browser profile policy, Browser lifecycle, tests, docs, changelog |
| Verdict | Green: accept |

**Findings:**

- Profile IDs match the server authority exactly and contain no account email or identity name.
- Profile directories remain byte-compatible with the existing Browser layout.
- A profile collision, context rebound, unsafe root, or malformed identity fails closed.
- Closing or failed startup releases only the exact bound context.
- Browser lifecycle no longer owns security policy, keeping pure policy independently testable.

## Cross-Task Findings

- Identity isolation does not establish durable cloud ownership or crash recovery.
- Browser distribution flags must remain disabled until signing, updater, recovery, and
  physical platform certification pass for an exact artifact.

## Build & Test Verification

```bash
(cd jobs/browser && npm test -- --run tests/browser-context-registry.test.ts) # 12 passed
(cd jobs/browser && npm run typecheck)                                        # passed
(cd jobs/browser && npm test)                                                 # 112 passed
(cd jobs/browser && npm run build)                                            # passed
```

## Overall Verdict

Green: **ACCEPT** - Ready to commit as a bounded identity-isolation slice. This
does not certify Browser distribution or employer-facing automation.

## Follow-ups for Next Batch

- Durable cloud-runner leases, heartbeats, and ambiguous-submit reconciliation.
- Exact-artifact local Browser packaging and cross-platform certification.
