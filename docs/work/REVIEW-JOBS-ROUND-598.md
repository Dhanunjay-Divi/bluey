# REVIEW: Round 598 - Jobs ATS Final-Submit Authority

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the changes against the
> current eligibility, evidence, runner and disabled-distribution contracts.

**Commit range:** working tree after `65019061`
**Reviewer:** Codex self-review
**Date:** 2026-08-03

## Per-Task Review

### Capability and execution authority

| Field | Value |
|-------|-------|
| Files | Capability registry, policy, execution wrapper, generic adapters and eligibility |
| Verdict | green - accept |

**Findings:**

- Only exact-version provider state machines can own final submission.
- Capability derives from exact parsed provider hosts on the server.
- Generic adapters can fill and validate but stop before final Submit.
- Visible confirmation language is not accepted as submission evidence.
- Unauthorized submitted outcomes are downgraded and audited.
- LinkedIn, Indeed, unknown and uncertified ATS paths remain review/handoff.

## Cross-Task Findings

- Queue-authority fixtures now persist the sponsorship preference they assert,
  avoiding false negatives from server-authoritative eligibility.
- Workday, Ashby and SmartRecruiters remain discoverable but are labeled
  `unknown_review`, matching their current execution capability.
- The two pre-existing untracked Browser contract scripts were not reviewed,
  staged or included.
- Production model generation, local/cloud Browser distribution and mailbox
  sync remain disabled.

## Build & Test Verification

```text
Automation tests                            227 passed
Automation TypeScript/build                  passed
Server unit tests                            829 passed
Server HTTP integration tests                 81 passed
Server additional integration tests            2 passed
Rust strict Clippy and formatting             passed
git diff --check                              passed
```

## Overall Verdict

Green: **ACCEPT** - Ready to commit on the feature branch. This closes the
source-level generic-submit defect; it does not certify or enable public
employer-facing automation.

## Follow-ups for Next Batch

- Add authorized live tenant certification for exact Greenhouse/Lever versions.
- Add crash-after-submit and side-effect-unknown production-like fault tests.
- Build provider-specific state machines before widening ATS authority.
